//! Per-frame scene assembly.
//!
//! Turns the navigation state + the (deterministically generated) worlds into
//! two instance lists for the renderer:
//!
//! * `opaque` — solid "preview cores" (miniatures of a sphere's own children,
//!   seen through its glass) and orbiting moons.
//! * `glass` — the translucent sphere shells, sorted back-to-front for correct
//!   alpha blending.
//!
//! It also does cursor ray-picking (which sphere are we pointing at) and the
//! hover "focus" factor that clears the glass on the sphere under the cursor.

use glam::Vec3;

use crate::camera::Camera;
use crate::config;
use crate::nav::Nav;
use crate::render::Instance;
use crate::world::{Sphere, World};

pub struct Frame {
    pub opaque: Vec<Instance>,
    pub glass: Vec<Instance>,
}

/// Build the instance lists for this frame. `cursor` is normalised device
/// coordinates ([-1,1], y up) of the mouse, used for the hover-clarity
/// affordance.
pub fn build(nav: &Nav, camera: &Camera, time: f32, cursor: (f32, f32)) -> Frame {
    let mut opaque = Vec::new();
    let mut glass = Vec::new();

    let dive = nav.eased();
    let eye = camera.eye(dive);
    let (ray_o, ray_d) = camera.ray(cursor.0, cursor.1);

    let outer = World::generate(nav.path());
    let transition = nav.transition();
    let target = transition.map(|t| t.target);

    // Cross-fades between the outer and inner worlds.
    let sibling_presence = 1.0 - smoothstep(dive, 0.0, 0.55); // outer fades out
    let inner_presence = smoothstep(dive, 0.45, 1.0); // inner fades in
    let shell_alpha = 1.0 - smoothstep(dive, 0.62, 1.0); // enveloping shell fades
    let shell_ease = smoothstep(dive, 0.0, 1.0); // grow + recentre

    for child in &outer.children {
        if Some(child.index) == target {
            // The target becomes the enveloping shell: recentre to the origin
            // and grow out past the camera. Its "contents" are the inner-world
            // children (added below), so it gets no preview cores.
            let center = child.rest_pos.lerp(Vec3::ZERO, shell_ease);
            let radius = lerp(child.radius, config::ENVELOP_RADIUS, shell_ease);
            glass.push(Instance::new(
                center,
                radius,
                child.tint,
                shell_alpha,
                dive, // evert amount drives the fold
                1.0,  // fully clear: we are entering it
                child.pattern,
                time * child.spin,
            ));
        } else {
            let presence = if transition.is_some() {
                sibling_presence
            } else {
                1.0
            };
            if presence > 0.02 {
                add_sphere(
                    &mut glass,
                    &mut opaque,
                    nav.path(),
                    child,
                    child.rest_pos,
                    presence,
                    eye,
                    ray_o,
                    ray_d,
                    time,
                );
            }
        }
    }

    // The inner world's children fade in at their resting positions, so by
    // t = 1 the view is exactly the resting view of the world we entered.
    if let Some(tg) = target {
        if inner_presence > 0.02 {
            let mut inner_path = nav.path().to_vec();
            inner_path.push(tg);
            let inner = World::generate(&inner_path);
            for child in &inner.children {
                add_sphere(
                    &mut glass,
                    &mut opaque,
                    &inner_path,
                    child,
                    child.rest_pos,
                    inner_presence,
                    eye,
                    ray_o,
                    ray_d,
                    time,
                );
            }
        }
    }

    // Back-to-front for the transparent pass.
    glass.sort_by(|a, b| {
        let da = a.center().distance_squared(eye);
        let db = b.center().distance_squared(eye);
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });

    Frame { opaque, glass }
}

/// Add one resting glass sphere plus its preview cores and moons.
#[allow(clippy::too_many_arguments)]
fn add_sphere(
    glass: &mut Vec<Instance>,
    opaque: &mut Vec<Instance>,
    world_path: &[usize],
    sphere: &Sphere,
    center: Vec3,
    presence: f32,
    eye: Vec3,
    ray_o: Vec3,
    ray_d: Vec3,
    time: f32,
) {
    let radius = sphere.radius * presence;

    // Hover clarity: how close the cursor ray passes to this sphere.
    let focus = hover_focus(ray_o, ray_d, center, sphere.radius);

    glass.push(Instance::new(
        center,
        radius,
        sphere.tint,
        presence, // alpha: fades the whole sphere in/out with presence
        0.0,      // not everting
        focus,
        sphere.pattern,
        time * sphere.spin,
    ));

    let _ = eye; // (reserved for future distance-based effects)

    // Preview cores: miniatures of this sphere's OWN children, at their real
    // relative positions, so the glass previews the world you would enter.
    let mut child_path = world_path.to_vec();
    child_path.push(sphere.index);
    let inner = World::generate(&child_path);
    for gc in inner.children.iter().take(config::PREVIEW_CORE_COUNT) {
        // gc.rest_pos = dir * LAYOUT_RADIUS, so dividing recovers the unit
        // direction; place it inside the parent at a shrunken radius.
        let rel = gc.rest_pos / config::LAYOUT_RADIUS;
        let pos = center + rel * (radius * config::PREVIEW_FILL);
        let r = radius * config::PREVIEW_CORE_RADIUS_FRAC;
        opaque.push(Instance::solid(pos, r, gc.tint));
    }

    // Orbiting moons — animated marks that aid at-a-glance identification.
    for m in &sphere.moons {
        let a = m.phase + time * m.speed;
        let (sa, ca) = a.sin_cos();
        let offset = Vec3::new(
            m.orbit_radius * ca,
            m.orbit_radius * sa * m.incline.cos(),
            m.orbit_radius * sa * m.incline.sin(),
        );
        opaque.push(Instance::solid(center + offset, m.size * presence, m.tint));
    }
}

/// Ray-pick the sphere under the cursor. Only meaningful while resting.
pub fn pick(nav: &Nav, camera: &Camera, ndc_x: f32, ndc_y: f32) -> Option<usize> {
    if nav.is_everting() {
        return None;
    }
    let (o, d) = camera.ray(ndc_x, ndc_y);
    let world = World::generate(nav.path());

    let mut best: Option<(f32, usize)> = None;
    for c in &world.children {
        if let Some(t) = ray_sphere(o, d, c.rest_pos, c.radius * 1.05) {
            if best.map_or(true, |(bt, _)| t < bt) {
                best = Some((t, c.index));
            }
        }
    }
    best.map(|(_, i)| i)
}

/// Nearest positive ray-sphere hit distance, if any.
fn ray_sphere(o: Vec3, d: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = o - center;
    let b = oc.dot(d);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t0 = -b - s;
    let t1 = -b + s;
    if t0 > 0.0 {
        Some(t0)
    } else if t1 > 0.0 {
        Some(t1)
    } else {
        None
    }
}

/// 1.0 when the cursor ray points straight at the sphere, falling off within a
/// few degrees — the "looking at it" clarity affordance.
fn hover_focus(o: Vec3, d: Vec3, center: Vec3, radius: f32) -> f32 {
    let to = center - o;
    let dist = to.length().max(0.0001);
    let cosang = (to / dist).dot(d).clamp(-1.0, 1.0);
    // Angular radius the sphere subtends, plus a little slack.
    let subtend = (radius / dist).atan();
    let ang = cosang.acos();
    // 1.0 when the ray is inside the sphere's silhouette, fading out beyond it.
    1.0 - smoothstep(ang, subtend * 0.6, subtend * 2.2)
}

fn smoothstep(x: f32, edge0: f32, edge1: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
