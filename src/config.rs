//! Central tunables for the Spheres prototype (Phase 0.6).
//!
//! Everything you are likely to want to tweak lives here so the rest of the
//! code reads cleanly. The README points here.

// --- The gesture ---------------------------------------------------------

/// Duration of a single eversion (enter or surface), in milliseconds.
/// Short and decisive (300-500ms) to avoid the vestibular tax.
pub const EVERSION_DURATION_MS: f32 = 440.0;

// --- The frame deadline --------------------------------------------------

/// Target frame budget in milliseconds. 16.6ms == 60Hz.
pub const FRAME_BUDGET_MS: f32 = 16.6;

/// How many seconds the rolling-MAX frame time looks back over.
pub const ROLLING_WINDOW_SECS: f32 = 3.0;

// --- Camera --------------------------------------------------------------

/// Camera distance when resting in the pulled-back global OVERVIEW (depth 0),
/// framing the whole graph so the structure is legible.
pub const OVERVIEW_DIST: f32 = 14.0;

/// Camera distance when resting INSIDE a node's local world (its neighbours).
pub const LOCAL_DIST: f32 = 7.0;

/// A small forward push during an eversion, zero at both ends (stable frame).
pub const CAMERA_PUSH: f32 = 1.2;

pub const CAMERA_FOV_DEG: f32 = 55.0;
pub const CAMERA_NEAR: f32 = 0.05;
pub const CAMERA_FAR: f32 = 400.0;

// --- Nodes ---------------------------------------------------------------

/// Base sphere radius for a node, before degree scaling.
pub const NODE_BASE_RADIUS: f32 = 0.62;

/// How much a node grows per unit of graph degree (so the hub reads bigger).
pub const NODE_DEGREE_SCALE: f32 = 0.07;

/// Radius the target node grows to when fully everted (envelops the camera).
pub const ENVELOP_RADIUS: f32 = 13.0;

/// Radius of the ring on which a node's neighbours are laid out in local view.
pub const LOCAL_LAYOUT_RADIUS: f32 = 3.4;

// --- Eversion deformation ------------------------------------------------

pub const FOLD_AMP: f32 = 0.13;
pub const FOLD_FREQ: f32 = 3.0;
pub const FOLD_TRAVEL: f32 = std::f32::consts::TAU;

// --- Glass material ------------------------------------------------------

pub const GLASS_OPACITY_EDGE: f32 = 0.82;
pub const GLASS_OPACITY_CENTER: f32 = 0.20;
pub const GLASS_FRESNEL_POWER: f32 = 2.5;
pub const GLASS_REFRACTION: f32 = 0.45;
/// Extra clarity when you point at a sphere — the browse affordance.
pub const GLASS_FOCUS_CLARITY: f32 = 0.55;
pub const GLASS_INNER_BOOST: f32 = 1.6;

// --- Kind materials (Requirement 2: kind legibility) ---------------------
//
// People are deliberately a warm magenta with extra glow so they read as
// materially different from the cool code nodes.

pub const TINT_PRODUCTION: [f32; 3] = [0.96, 0.56, 0.16];
pub const TINT_LIBRARY: [f32; 3] = [0.26, 0.56, 0.95];
pub const TINT_TOOL: [f32; 3] = [0.22, 0.82, 0.46];
pub const TINT_PERSON: [f32; 3] = [0.97, 0.34, 0.72];

/// Per-kind self-glow added to the body (people glow most → "present").
pub const GLOW_PRODUCTION: f32 = 0.20;
pub const GLOW_LIBRARY: f32 = 0.10;
pub const GLOW_TOOL: f32 = 0.12;
pub const GLOW_PERSON: f32 = 0.34;

// --- Rings (Requirement 3: data-driven ring readout) ---------------------

/// Inner / outer ring radius as a multiple of the node radius.
pub const RING_INNER_FRAC: f32 = 1.35;
pub const RING_OUTER_FRAC: f32 = 1.95;

/// Saturn-like tilt of the rings, in degrees.
pub const RING_TILT_DEG: f32 = 68.0;

/// Number of marker segments (glowing squares) around the ring.
pub const RING_MARKER_COUNT: u32 = 4;

/// Ring base brightness and lit-marker brightness.
pub const RING_BASE_GLOW: f32 = 0.18;
pub const RING_MARKER_GLOW: f32 = 1.0;

// --- Edges (Requirement 1: visible structure) ----------------------------

/// Ribbon half-width of an edge link, in world units.
pub const EDGE_WIDTH: f32 = 0.035;

/// Base edge brightness, and the boost applied to edges of the hovered node
/// (browse: peek at a node's relations).
pub const EDGE_GLOW: f32 = 0.70;
pub const EDGE_HOVER_BOOST: f32 = 2.2;

/// Ownership edges (people → things) are drawn brighter/wider so a person's
/// span across the graph is the showpiece (Requirement 6).
pub const EDGE_OWNS_BOOST: f32 = 1.7;

// --- Ambience (Requirement 7: contexts feel distinct) --------------------

/// Background in the neutral global overview.
pub const CLEAR_OVERVIEW: [f64; 4] = [0.012, 0.013, 0.022, 1.0];

/// How strongly the current node's kind tints the background when inside it.
pub const AMBIENT_TINT_STRENGTH: f64 = 0.05;

// --- Sphere tessellation -------------------------------------------------

pub const SPHERE_RINGS: u32 = 32;
pub const SPHERE_SEGMENTS: u32 = 64;

/// World-space direction *towards* the light.
pub const LIGHT_DIR: [f32; 3] = [0.4, 0.8, 0.55];
