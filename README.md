# Spheres — Phase 0.5

A standalone [wgpu](https://wgpu.rs/) + [winit](https://github.com/rust-windowing/winit)
prototype of the **recursive** sphere desktop. Glassy dark spheres hang in
space, each one previewing the little world inside it. Click a sphere and it
**everts and envelops you** — you arrive inside it, in a new space of child
spheres, each enterable the same way, to any depth. Esc surfaces you back out
one level.

This is **not** a compositor. No Smithay, no Wayland plumbing, no real apps. It
exists to answer the two questions Phase 0.5 gates (see `docs/DESIGN.md`):

1. *Does the recursion feel coherent* — does "everything is a sphere, entering
   is everting" hold up nested several levels deep?
2. *Is the world legible and navigable* — can you tell N featureless spheres
   apart, and is getting back out rock-solid so you never get lost?

The frame-time HUD from Phase 0 is retained: the eversion is still the
worst-case frame, and the scene is now heavy (glass overdraw + many spheres),
so this is the realistic worst case to watch.

## Run

```sh
cargo run --release
```

(`--release` matters for representative frame times.)

For adapter / device logging: `RUST_LOG=info cargo run --release`.

## Controls

| Input | Action |
|---|---|
| **Left click** | Evert into the sphere under the cursor |
| **Right click** or **Esc** | Surface out one level (reliable from any state) |
| **Q** / window close | Quit |

Pointing at a sphere clears its glass (the "invites entry" affordance).
Triggering a gesture mid-animation just reverses it — Esc always takes you
*up*, from resting or mid-dive, so the way out is always available.

## What you are looking at

- **Glassy dark spheres.** Blackish translucent glass with a coloured fresnel
  rim. You see *through* them to the bright **preview cores** inside — miniatures
  of that sphere's actual children, at their real relative positions. A sphere
  literally previews the world you would enter.
- **Distinct at a glance.** Each sphere in a world has its own hue (by index, so
  siblings always differ), a faint surface pattern, and 1–3 orbiting moons.
- **Recursion.** Entering a sphere drops you into a fresh world of its children.
  Worlds are generated deterministically from the path, so positions are
  **stable** — leave and come back and everything is where you left it (spatial
  memory). Depth is unbounded.
- **Breadcrumb.** Bottom-left, always on: `depth: 2   Root > Amber > Azure`, so
  you always know where you are and how you got there.

## The HUD

Top-left, always on (unchanged from Phase 0):

- **frame** / **fps** — last frame time and frame rate.
- **max (3s)** — rolling maximum frame time, so a brief spike does not scroll
  away before you see it.
- **budget** — the target; the readout turns **red** on any frame over it.
- **eversion (last) / EVERSION (live)** — frames that blew the budget during the
  most recent eversion. The eversion in a populated scene is the worst-case
  frame, so this is the headline number.

Frame time is measured vsync-off (`PresentMode::AutoNoVsync`) so the number
reflects real work, not the present-block.

## Debug / headless helpers (env vars)

| Variable | Effect |
|---|---|
| `SPHERES_AUTODEMO=1` | Dive and surface on a timer (no mouse) — for hands-free viewing / capture. |
| `SPHERES_PERFLOG=1` | Log frame stats to stderr periodically (the HUD numbers, without reading the window). |
| `SPHERES_CAPTURE=path` | Render a few scripted frames to `path.root.ppm`, `path.inside.ppm`, `path.evert.ppm` and exit — a compositor-independent way to see what the GPU drew. |

## Constants to tune

All tunables live in [`src/config.rs`]. The ones you will reach for first:

| Constant | Meaning |
|---|---|
| `EVERSION_DURATION_MS` | Length of the gesture (default 420 ms; keep it 300–500, fast and decisive). |
| `FRAME_BUDGET_MS` | The deadline the HUD highlights against (default 16.6 = 60 Hz). |
| `SPHERES_PER_LEVEL` | Child spheres per world (default 8). |
| `CAMERA_REST_DIST` | How far back the resting camera sits. |
| `LAYOUT_RADIUS` / `CHILD_RADIUS` | World layout spread and sphere size. |
| `ENVELOP_RADIUS` | How large the target grows to swallow you. |
| `GLASS_OPACITY_CENTER` / `GLASS_OPACITY_EDGE` | Glass transparency face-on vs at the rim. |
| `GLASS_FRESNEL_POWER` / `GLASS_REFRACTION` | Rim falloff and cosmetic shimmer. |
| `GLASS_FOCUS_CLARITY` | How much pointing at a sphere clears its glass. |
| `GLASS_INNER_BOOST` | Brightening of the inner surface once you are enclosed. |
| `PREVIEW_CORE_COUNT` / `PREVIEW_FILL` | The inner-world preview inside each sphere. |
| `SPHERE_RINGS` / `SPHERE_SEGMENTS` | Tessellation — lower these if the iGPU struggles. |

## Module map

| File | Responsibility |
|---|---|
| `src/main.rs` | Window, event loop, input, cursor, per-frame timing, capture. |
| `src/config.rs` | All tunable constants. |
| `src/world.rs` | The deterministic nested-world model (spheres, palette, RNG). |
| `src/nav.rs` | The navigation state machine (path + transitions, Esc reliability). |
| `src/scene.rs` | Per-frame instance assembly, preview cores/moons, ray-picking. |
| `src/camera.rs` | Camera pose, eversion bob, ray unprojection for picking. |
| `src/eversion.rs` | The eased progress scalar driving a gesture. |
| `src/render.rs` | wgpu setup, the opaque + glass pipelines, instancing, the draw. |
| `src/hud.rs` | Frame-time stats + breadcrumb, via glyphon. |
| `src/shader.wgsl` | Instanced eversion deformation + glass / solid shading. |
