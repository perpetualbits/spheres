//! Central tunables for the Spheres prototype (Phase 0.5).
//!
//! Everything you are likely to want to tweak lives here so the rest of the
//! code reads cleanly. The README points here.

// --- The gesture ---------------------------------------------------------

/// Duration of a single eversion (enter or surface), in milliseconds.
///
/// Deliberately short (300-500ms). Fast and decisive avoids the vestibular tax
/// of an interaction triggered hundreds of times a day, and fewer in-flight
/// frames means fewer worst-case frames to keep in budget.
pub const EVERSION_DURATION_MS: f32 = 420.0;

// --- The frame deadline --------------------------------------------------

/// Target frame budget in milliseconds. 16.6ms == 60Hz. The HUD turns red and
/// the over-budget counter ticks whenever a frame exceeds this.
pub const FRAME_BUDGET_MS: f32 = 16.6;

/// How many seconds the rolling-MAX frame time looks back over.
pub const ROLLING_WINDOW_SECS: f32 = 3.0;

// --- The world: how many spheres, how deep -------------------------------

/// Child spheres per world. The legibility test: can N featureless spheres be
/// told apart at a glance? (Distinct hue per index, plus pattern and moons.)
pub const SPHERES_PER_LEVEL: usize = 8;

/// How deep the auto-demo (SPHERES_AUTODEMO=1) will dive before surfacing.
/// The world itself is infinitely deep; this only bounds the demo.
pub const AUTODEMO_MAX_DEPTH: usize = 3;

// --- Camera --------------------------------------------------------------

/// Distance of the resting camera from the world centre. Must be large enough
/// to see the whole cluster of child spheres.
pub const CAMERA_REST_DIST: f32 = 7.0;

/// A small forward push during an eversion, for kinetic life. Zero at both
/// ends of the gesture so the resting reference frame is stable (low nausea).
pub const CAMERA_PUSH: f32 = 1.4;

/// Vertical field of view, in degrees.
pub const CAMERA_FOV_DEG: f32 = 55.0;

/// Near/far planes. Near is tiny so passing through the enveloping shell does
/// not clip harshly as it sweeps past the eye.
pub const CAMERA_NEAR: f32 = 0.05;
pub const CAMERA_FAR: f32 = 200.0;

// --- World layout --------------------------------------------------------

/// Radius of the shell the child spheres are scattered over (centred on the
/// world origin, in front of the camera).
pub const LAYOUT_RADIUS: f32 = 3.2;

/// Base radius of a child sphere (varied slightly per sphere).
pub const CHILD_RADIUS: f32 = 0.85;

/// Radius the target sphere grows to when fully everted — large enough to
/// comfortably enclose the camera at `CAMERA_REST_DIST`.
pub const ENVELOP_RADIUS: f32 = 11.0;

// --- Eversion deformation (the fold that reads as "inside-out") -----------

pub const FOLD_AMP: f32 = 0.14;
pub const FOLD_FREQ: f32 = 3.0;
pub const FOLD_TRAVEL: f32 = std::f32::consts::TAU;

// --- Glass material ------------------------------------------------------

/// Opacity at the grazing edge (fresnel) — fairly high, so the sphere's
/// silhouette reads as a solid glass object.
pub const GLASS_OPACITY_EDGE: f32 = 0.82;

/// Opacity face-on at the centre — low, so the dark glass reads as see-through
/// and the bright preview cores inside show clearly.
pub const GLASS_OPACITY_CENTER: f32 = 0.20;

/// Fresnel falloff power (higher = thinner bright rim, more see-through face).
pub const GLASS_FRESNEL_POWER: f32 = 2.5;

/// Fake refraction strength: how much the inner preview is colour-shifted /
/// the surface shimmers. Cosmetic glassiness knob.
pub const GLASS_REFRACTION: f32 = 0.45;

/// How much extra clarity (transparency) a sphere gains when you look straight
/// at it / are close — the "invites entry" affordance. 0 = none, 1 = strong.
pub const GLASS_FOCUS_CLARITY: f32 = 0.55;

/// Brightening applied to the inner surface once you are enclosed, so being
/// inside a world reads warmer/closer than looking at it from outside.
pub const GLASS_INNER_BOOST: f32 = 1.6;

// --- Inner-world previews (the "see your children inside") ----------------

/// How many of a sphere's children to preview as cores inside it. Keep this
/// equal to `SPHERES_PER_LEVEL` so the preview is a *faithful* miniature — the
/// dots you see inside a sphere are exactly the spheres you find on entering,
/// at the same relative positions. Lower it only if you deliberately want a
/// partial hint.
pub const PREVIEW_CORE_COUNT: usize = SPHERES_PER_LEVEL;

/// Fraction of the parent's radius the mini preview layout occupies.
pub const PREVIEW_FILL: f32 = 0.5;

/// Preview core radius as a fraction of the parent sphere's radius.
pub const PREVIEW_CORE_RADIUS_FRAC: f32 = 0.13;

// --- Sphere tessellation -------------------------------------------------

/// Tessellation. The scene is now many spheres; this is moderate so the
/// populated eversion is a real but survivable stress on the frame budget.
/// Raise to push harder, lower if the iGPU struggles.
pub const SPHERE_RINGS: u32 = 40; // latitude divisions
pub const SPHERE_SEGMENTS: u32 = 80; // longitude divisions

// --- Look ----------------------------------------------------------------

/// Dark, simple background.
pub const CLEAR_COLOR: [f64; 4] = [0.015, 0.015, 0.028, 1.0];

/// World-space direction *towards* the light.
pub const LIGHT_DIR: [f32; 3] = [0.4, 0.8, 0.55];
