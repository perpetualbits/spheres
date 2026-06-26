//! The world model: a deterministic tree of nested worlds.
//!
//! A world is *not stored* — it is generated on demand from its path (the
//! sequence of child indices taken from the root). Because generation is a pure
//! function of the path, every arrangement is reproducible: leave a world and
//! come back and the spheres are exactly where they were. That stability is
//! what makes a place become familiar (spatial memory), and it lets the world
//! be infinitely deep without storing anything.

use glam::Vec3;

use crate::config;

/// A named colour. Children are coloured by their index, so siblings always
/// differ and the breadcrumb can name the path ("Root > Amber > Azure").
pub struct Hue {
    pub name: &'static str,
    pub rgb: [f32; 3],
}

/// The palette, indexed by child position within a world.
pub const PALETTE: &[Hue] = &[
    Hue { name: "Crimson", rgb: [0.91, 0.23, 0.28] },
    Hue { name: "Amber",   rgb: [0.95, 0.62, 0.16] },
    Hue { name: "Emerald", rgb: [0.20, 0.80, 0.46] },
    Hue { name: "Azure",   rgb: [0.24, 0.55, 0.95] },
    Hue { name: "Violet",  rgb: [0.62, 0.36, 0.92] },
    Hue { name: "Rose",    rgb: [0.95, 0.40, 0.66] },
    Hue { name: "Teal",    rgb: [0.18, 0.74, 0.78] },
    Hue { name: "Gold",    rgb: [0.86, 0.78, 0.22] },
];

/// Name of the hue at a given child index (wraps if there are more spheres
/// than palette entries).
pub fn color_name(index: usize) -> &'static str {
    PALETTE[index % PALETTE.len()].name
}

/// RGB of the hue at a given child index.
pub fn color_rgb(index: usize) -> Vec3 {
    Vec3::from(PALETTE[index % PALETTE.len()].rgb)
}

/// One orbiting moon — a small animating mark that helps tell spheres apart at
/// a glance.
#[derive(Clone, Copy)]
pub struct Moon {
    pub orbit_radius: f32,
    pub speed: f32,
    pub phase: f32,
    pub incline: f32,
    pub size: f32,
    pub tint: Vec3,
}

/// A child sphere within a world.
#[derive(Clone)]
pub struct Sphere {
    pub index: usize,
    /// Resting position in the world (centred on the origin, in front of the
    /// camera). Stable across visits.
    pub rest_pos: Vec3,
    pub radius: f32,
    pub tint: Vec3,
    /// Surface band frequency, for a faint distinguishing pattern.
    pub pattern: f32,
    /// Self-rotation speed (radians/sec) of the surface pattern.
    pub spin: f32,
    pub moons: Vec<Moon>,
}

/// A world: just its child spheres.
pub struct World {
    pub children: Vec<Sphere>,
}

impl World {
    /// Generate the world reached by following `path` from the root.
    pub fn generate(path: &[usize]) -> World {
        let count = config::SPHERES_PER_LEVEL;
        let seed = seed_of(path);
        let mut children = Vec::with_capacity(count);

        for i in 0..count {
            // Per-child deterministic stream.
            let mut rng = Rng::new(seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

            // Even spread over a shell (fibonacci sphere) + a little jitter, so
            // the arrangement is legible but not mechanical.
            let base = fibonacci_point(i, count);
            let jitter = Vec3::new(
                rng.signed() * 0.18,
                rng.signed() * 0.18,
                rng.signed() * 0.18,
            );
            let dir = (base + jitter).normalize_or_zero();
            let rest_pos = dir * config::LAYOUT_RADIUS;

            let radius = config::CHILD_RADIUS * (0.8 + rng.unit() * 0.45);
            let tint = color_rgb(i);
            let pattern = 4.0 + rng.unit() * 8.0;
            let spin = (rng.signed()) * 0.8;

            let moon_count = 1 + (rng.next() % 3) as usize; // 1..=3
            let mut moons = Vec::with_capacity(moon_count);
            for _ in 0..moon_count {
                moons.push(Moon {
                    orbit_radius: radius * (1.5 + rng.unit() * 0.8),
                    speed: (0.4 + rng.unit() * 1.1) * if rng.next() & 1 == 0 { 1.0 } else { -1.0 },
                    phase: rng.unit() * std::f32::consts::TAU,
                    incline: rng.unit() * std::f32::consts::PI,
                    size: radius * (0.12 + rng.unit() * 0.08),
                    // Moons take a slightly shifted tint so they read as "of"
                    // the sphere but distinct from its body.
                    tint: (tint * 0.5 + Vec3::splat(0.5)).min(Vec3::ONE),
                });
            }

            children.push(Sphere {
                index: i,
                rest_pos,
                radius,
                tint,
                pattern,
                spin,
                moons,
            });
        }

        World { children }
    }
}

/// Mixing the path indices into a fixed root seed gives every world a stable,
/// reproducible seed.
fn seed_of(path: &[usize]) -> u64 {
    let mut s = 0x5048_4552_4553_0001u64; // fixed root seed
    for &idx in path {
        s ^= (idx as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
        s = splitmix64(&mut s);
    }
    s
}

/// The i-th of `n` points evenly spread on a unit sphere (fibonacci spiral).
fn fibonacci_point(i: usize, n: usize) -> Vec3 {
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let theta = i as f32 * golden;
    Vec3::new(r * theta.cos(), y, r * theta.sin())
}

// --- Deterministic RNG (splitmix64) --------------------------------------

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Rng {
            state: seed ^ 0xDEAD_BEEF_CAFE_F00D,
        }
    }
    fn next(&mut self) -> u64 {
        splitmix64(&mut self.state)
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform in [-1, 1).
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}
