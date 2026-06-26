// Instanced sphere shader.
//
// One unit-sphere mesh, drawn many times. Per-instance data places, sizes,
// tints and (for the everting target) deforms each sphere. Two fragment
// entry points share the vertex stage: `fs_glass` for the translucent shells,
// `fs_solid` for the opaque preview cores and moons inside them.

const PI: f32 = 3.14159265358979;

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>, // xyz eye, w = time (unused here)
    light_dir: vec4<f32>,  // xyz: direction TO light
    fold: vec4<f32>,       // amp, freq, travel, _
    glass: vec4<f32>,      // opacity_center, opacity_edge, fresnel_power, refraction
    glass2: vec4<f32>,     // focus_clarity, inner_boost, _, _
};

@group(0) @binding(0)
var<uniform> g: Globals;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tint: vec3<f32>,
    @location(3) alpha: f32,
    @location(4) focus: f32,
    @location(5) pattern: f32,
    // 1.0 when the camera is actually enclosed by this sphere (vs just looking
    // at its far face). Flat: it is a per-instance property.
    @location(6) @interpolate(flat) enclosed: f32,
};

@vertex
fn vs_main(
    @location(0) dir_in: vec3<f32>,
    @location(1) center_radius: vec4<f32>,
    @location(2) tint_alpha: vec4<f32>,
    @location(3) params: vec4<f32>,
) -> VsOut {
    let center = center_radius.xyz;
    let radius = center_radius.w;
    let evert = params.x;
    let focus = params.y;
    let pattern = params.z;
    let spin = params.w;

    // Spin the surface about Y (animates the pattern; harmless for tiny cores).
    let cs = cos(spin);
    let sn = sin(spin);
    let d0 = normalize(dir_in);
    let d = vec3<f32>(d0.x * cs + d0.z * sn, d0.y, -d0.x * sn + d0.z * cs);

    // Eversion fold — only active when evert > 0 (the target shell). A wave
    // travelling along the camera axis that reads as the surface turning
    // inside-out as it grows past the eye.
    let ang = acos(clamp(d.z, -1.0, 1.0));
    let amp = g.fold.x * sin(PI * evert);
    let wave = sin(g.fold.y * ang - g.fold.z * evert);
    let r = radius * (1.0 + amp * wave);

    let world = center + d * r;

    var out: VsOut;
    out.world_pos = world;
    out.normal = d; // radial normal
    out.tint = tint_alpha.xyz;
    out.alpha = tint_alpha.w;
    out.focus = focus;
    out.pattern = pattern;
    // Enclosed when the eye is inside this sphere's (current) radius.
    let to_eye = g.camera_pos.xyz - center;
    out.enclosed = select(0.0, 1.0, dot(to_eye, to_eye) < radius * radius);
    out.clip_pos = g.view_proj * vec4<f32>(world, 1.0);
    return out;
}

// --- Glass shells --------------------------------------------------------

@fragment
fn fs_glass(in: VsOut) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    let view_dir = normalize(g.camera_pos.xyz - in.world_pos);
    let facing = dot(n, view_dir);
    let enclosed = in.enclosed > 0.5; // camera actually inside this sphere

    // Fresnel: bright, opaque rim at grazing angles; clear face-on.
    let fres = pow(1.0 - abs(facing), g.glass.z);
    var opacity = mix(g.glass.x, g.glass.y, fres);
    // Looking straight at a sphere clears its glass, inviting entry.
    opacity = opacity * (1.0 - in.focus * g.glass2.x);

    // Flip the normal to face us so whichever surface we see is lit correctly.
    if (facing < 0.0) {
        n = -n;
    }

    let l = normalize(g.light_dir.xyz);
    let diff = max(dot(n, l), 0.0);

    // Faint latitude bands so each sphere has a little surface character.
    let lat = asin(clamp(n.y, -1.0, 1.0));
    let band = 0.92 + 0.08 * sin(in.pattern * lat * 2.0);

    var body: vec3<f32>;
    if (enclosed) {
        // We are enclosed: the inner surface IS the world we are in, so it
        // reads clearly coloured (and brighter, per the inner-boost knob).
        let base = min(in.tint * g.glass2.y, vec3<f32>(1.0));
        body = base * (0.30 + 0.60 * diff) * band;
    } else {
        // From outside it is dark, blackish glass — only a faint hint of body,
        // so the bright preview cores inside show through clearly.
        body = in.tint * (0.03 + 0.16 * diff) * band;
    }

    // Coloured fresnel rim carries the hue identity even when the body is dark.
    let rim = fres * (0.5 + g.glass.w);
    let col = body + in.tint * rim;

    return vec4<f32>(col, clamp(opacity, 0.0, 1.0) * in.alpha);
}

// --- Solid preview cores / moons -----------------------------------------

@fragment
fn fs_solid(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(g.light_dir.xyz);
    let diff = max(dot(n, l), 0.0);
    // A little self-glow so they read clearly through the surrounding glass.
    let col = in.tint * (0.45 + 0.55 * diff) + in.tint * 0.25;
    return vec4<f32>(col, 1.0);
}
