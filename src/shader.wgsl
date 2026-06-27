// Spheres Phase 0.6 shaders.
//
// Shared globals + four programs over three meshes:
//   * sphere  — instanced glass nodes (vs_main + fs_glass), also used by the
//               depth pre-pass (vs_main, no fragment).
//   * ring     — instanced Saturn-ring readouts (vs_ring + fs_ring).
//   * edge     — instanced camera-facing link ribbons (vs_edge + fs_edge).

const PI: f32 = 3.14159265358979;

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>, // xyz eye, w time
    light_dir: vec4<f32>,
    fold: vec4<f32>,       // amp, freq, travel, _
    glass: vec4<f32>,      // opacity_center, opacity_edge, fresnel_power, refraction
    glass2: vec4<f32>,     // focus_clarity, inner_boost, _, _
};

@group(0) @binding(0)
var<uniform> g: Globals;

// =========================================================================
// Sphere (glass nodes)
// =========================================================================

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tint: vec3<f32>,
    @location(3) alpha: f32,
    @location(4) focus: f32,
    @location(5) glow: f32,
    @location(6) @interpolate(flat) enclosed: f32,
};

@vertex
fn vs_main(
    @location(0) dir_in: vec3<f32>,
    @location(1) center_radius: vec4<f32>,
    @location(2) tint_alpha: vec4<f32>,
    @location(3) params: vec4<f32>, // evert, focus, glow, spin
) -> VsOut {
    let center = center_radius.xyz;
    let radius = center_radius.w;
    let evert = params.x;
    let spin = params.w;

    let cs = cos(spin);
    let sn = sin(spin);
    let d0 = normalize(dir_in);
    let d = vec3<f32>(d0.x * cs + d0.z * sn, d0.y, -d0.x * sn + d0.z * cs);

    let ang = acos(clamp(d.z, -1.0, 1.0));
    let amp = g.fold.x * sin(PI * evert);
    let wave = sin(g.fold.y * ang - g.fold.z * evert);
    let r = radius * (1.0 + amp * wave);
    let world = center + d * r;

    var out: VsOut;
    out.world_pos = world;
    out.normal = d;
    out.tint = tint_alpha.xyz;
    out.alpha = tint_alpha.w;
    out.focus = params.y;
    out.glow = params.z;
    let to_eye = g.camera_pos.xyz - center;
    out.enclosed = select(0.0, 1.0, dot(to_eye, to_eye) < radius * radius);
    out.clip_pos = g.view_proj * vec4<f32>(world, 1.0);
    return out;
}

@fragment
fn fs_glass(in: VsOut) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    let view_dir = normalize(g.camera_pos.xyz - in.world_pos);
    let facing = dot(n, view_dir);
    let enclosed = in.enclosed > 0.5;

    let fres = pow(1.0 - abs(facing), g.glass.z);
    var opacity = mix(g.glass.x, g.glass.y, fres);
    opacity = opacity * (1.0 - in.focus * g.glass2.x);

    if (facing < 0.0) {
        n = -n;
    }

    let l = normalize(g.light_dir.xyz);
    let diff = max(dot(n, l), 0.0);

    var body: vec3<f32>;
    if (enclosed) {
        let base = min(in.tint * g.glass2.y, vec3<f32>(1.0));
        body = base * (0.30 + 0.60 * diff);
    } else {
        body = in.tint * (0.05 + 0.32 * diff);
    }
    // Per-kind self-glow (people glow most → materially "present").
    body = body + in.tint * in.glow;

    let rim = fres * (0.55 + g.glass.w);
    let col = body + in.tint * rim;
    return vec4<f32>(col, clamp(opacity, 0.0, 1.0) * in.alpha);
}

// =========================================================================
// Ring readout (data-driven markers)
// =========================================================================

struct RingOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) tint: vec3<f32>,
    @location(1) angle: f32,   // 0..1 around the ring
    @location(2) radial: f32,  // 0 inner .. 1 outer
    @location(3) bits: f32,
    @location(4) marker_count: f32,
    @location(5) base_glow: f32,
    @location(6) marker_glow: f32,
    @location(7) alpha: f32,
};

@vertex
fn vs_ring(
    @location(0) vert: vec3<f32>, // (cos a, sin a, side)
    @location(1) center_inner: vec4<f32>, // xyz center, w inner radius
    @location(2) tint_outer: vec4<f32>,   // rgb tint, w outer radius
    @location(3) marker: vec4<f32>,        // bits, marker_count, base_glow, marker_glow
    @location(4) extra: vec4<f32>,         // alpha, tilt, _, _
) -> RingOut {
    let ca = vert.x;
    let sa = vert.y;
    let side = vert.z;
    let radius = mix(center_inner.w, tint_outer.w, side);

    // Ring lies in the XZ plane, then tilted about X (Saturn-like).
    let local = vec3<f32>(ca * radius, 0.0, sa * radius);
    let tilt = extra.y;
    let ct = cos(tilt);
    let st = sin(tilt);
    let tilted = vec3<f32>(local.x, local.y * ct - local.z * st, local.y * st + local.z * ct);
    let world = center_inner.xyz + tilted;

    var out: RingOut;
    out.clip_pos = g.view_proj * vec4<f32>(world, 1.0);
    out.tint = tint_outer.xyz;
    out.angle = (atan2(sa, ca) / (2.0 * PI)) + 0.5; // 0..1
    out.radial = side;
    out.bits = marker.x;
    out.marker_count = marker.y;
    out.base_glow = marker.z;
    out.marker_glow = marker.w;
    out.alpha = extra.x;
    return out;
}

@fragment
fn fs_ring(in: RingOut) -> @location(0) vec4<f32> {
    let count = max(in.marker_count, 1.0);
    let seg_f = in.angle * count;
    let seg = u32(floor(seg_f)) % u32(count);
    let within_seg = fract(seg_f); // 0..1 across the segment angularly

    var glow = in.base_glow;

    // A glowing "square": the centre patch of a segment, angularly and
    // radially, lit only when this segment's bit is set.
    let bits = u32(in.bits + 0.5);
    let lit = ((bits >> seg) & 1u) == 1u;
    let in_square =
        within_seg > 0.28 && within_seg < 0.72 && in.radial > 0.3 && in.radial < 0.7;
    if (lit && in_square) {
        glow = in.marker_glow;
    }

    let col = in.tint * glow;
    return vec4<f32>(col * in.alpha, 1.0); // additive
}

// =========================================================================
// Edge links (camera-facing ribbons)
// =========================================================================

struct EdgeOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) across: f32,
    @location(1) color: vec3<f32>,
    @location(2) glow: f32,
};

@vertex
fn vs_edge(
    @location(0) quad: vec2<f32>, // (along 0..1, across -1..1)
    @location(1) a_width: vec4<f32>, // A.xyz, half_width
    @location(2) b_glow: vec4<f32>,  // B.xyz, glow
    @location(3) color_alpha: vec4<f32>,
) -> EdgeOut {
    let a = a_width.xyz;
    let b = b_glow.xyz;
    let p = mix(a, b, quad.x);
    let dir = normalize(b - a);
    let view = normalize(g.camera_pos.xyz - p);
    var perp = cross(dir, view);
    let plen = length(perp);
    if (plen < 1e-4) {
        perp = vec3<f32>(0.0, 1.0, 0.0);
    } else {
        perp = perp / plen;
    }
    let world = p + perp * (quad.y * a_width.w);

    var out: EdgeOut;
    out.clip_pos = g.view_proj * vec4<f32>(world, 1.0);
    out.across = quad.y;
    out.color = color_alpha.xyz;
    out.glow = b_glow.w * color_alpha.w;
    return out;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    // Bright down the centreline, fading to the edges.
    let intensity = pow(1.0 - abs(in.across), 1.6) * in.glow;
    return vec4<f32>(in.color * intensity, 1.0); // additive
}
