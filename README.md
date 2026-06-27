# Spheres — Phase 0.6

A standalone [wgpu](https://wgpu.rs/) + [winit](https://github.com/rust-windowing/winit)
prototype of the spatial/relational interface. NOT a compositor. This phase
hand-authors a real **graph** of the "eno" project and makes its *structure* a
place you can be inside of — revealing relationships a file tree hides: `core`'s
centrality, the hidden `carve`/`glint` cross-couplings, and one person spanning
libraries and tools at once.

Every node is a glassy sphere (a container you evert into) wearing a **Saturn
ring** that reads out its data. Nodes are linked by glowing **edges**, so the
whole graph is visible at a glance. Crucially, nodes are keyed by a **stable
id**: reaching one two ways lands you in the *same* world.

## Run

```sh
cargo run --release
```

`RUST_LOG=info cargo run --release` logs the adapter.

## Controls

| Input | Action |
|---|---|
| **Left click** | Evert into the node under the cursor |
| **Right click** / **Esc** | Surface out one level (reliable from any state) |
| **Q** / window close | Quit |

Point at a node to **browse** it: its glass clears and its relation edges
brighten (peek before you commit). Everting **enters** the node's world, where
its neighbours surround you.

## What you are looking at

- **The overview (depth 0).** The whole graph, pulled back. `core` sits at the
  centre with five libraries' `depends-on` edges converging on it (it is also
  the largest, scaled by degree). Production fans `uses` edges down from the
  top; tools sit to the sides, each throwing a link into two worlds; people
  float in a front shell, their `owns` edges (brightest) raking across the
  library/tool boundary — **Roland** owns `core`, `crest`, `carve`, `glint`,
  the edge a file tree cannot show.
- **A node's world.** Evert into a node and its neighbours lay out around you,
  edges radiating from the centre, each named (`depends-on`, `uses`, `owns`…).
- **Kind legibility.** Production = amber, library = blue, tool = green,
  person = magenta (and people glow brightest — materially "present").
- **Ring readouts.** Each sphere's tilted ring shows up to four glowing-square
  markers, **derived from the graph**: owned · depends-on-core · hub
  (in-degree ≥ 3) · cross-cutting (touches ≥ 3 kinds). Name + kind ride as a
  billboarded label.
- **Convergence.** Enter `core` from `crest`, from `io`, from production — the
  same world every time (same neighbours, same positions). The breadcrumb
  trail differs; the place does not.

## The graph (`src/graph.rs`)

Nodes by kind:

| kind | nodes |
|---|---|
| production | `desert-monument` |
| library | `crest`, `siftr`, `fx`, `gfx`, `io`, `core` |
| tool | `carve`, `glint`, `smolr` |
| person | `Roland`, `Simon`, `Elise`, `Segher` |

Edges (directed, named):

```
desert-monument  --uses-->        crest, siftr, fx, gfx, core, io
desert-monument  --packed-by-->   smolr
crest,siftr,fx,gfx,io --depend-on--> core
siftr            --feeds-->       fx
carve            --authors-for--> crest, desert-monument
glint            --shaders-for--> gfx, desert-monument
Roland           --owns-->        core, crest, carve, glint
Simon            --owns-->        siftr, fx
Elise            --feeds-->       io, siftr
Segher           --owns-->        smolr
```

## The HUD

Top-left, always on: frame time / fps, rolling 3s max, the budget (red on any
over-budget frame), and the per-eversion over-budget count. Bottom-left: the
`depth: N   overview > … ` breadcrumb. Measured vsync-off so spikes show.

## Debug / headless helpers (env vars)

| Variable | Effect |
|---|---|
| `SPHERES_AUTODEMO=1` | Walk overview → production → core and back, on a timer. |
| `SPHERES_PERFLOG=1` | Log frame stats to stderr periodically. |
| `SPHERES_CAPTURE=path` | Render scripted frames (`overview`, `production`, `core-via-production`, `core-via-io`, `evert`) to `path.*.ppm` and exit. The two `core-*` frames are byte-identical — convergence, made visible. |

## Constants to tune (`src/config.rs`)

| Constant | Meaning |
|---|---|
| `EVERSION_DURATION_MS` | Gesture length (default 440 ms). |
| `FRAME_BUDGET_MS` | HUD deadline (16.6 = 60 Hz). |
| `OVERVIEW_DIST` / `LOCAL_DIST` | Camera distance pulled-back vs inside a node. |
| `TINT_*` / `GLOW_*` | Per-kind material and self-glow. |
| `RING_*` | Ring size, tilt, marker count, glow. |
| `EDGE_WIDTH` / `EDGE_GLOW` / `EDGE_OWNS_BOOST` / `EDGE_HOVER_BOOST` | Link look and emphasis. |
| `NODE_BASE_RADIUS` / `NODE_DEGREE_SCALE` | Node size and how much the hub grows. |
| `GLASS_*` | Glass transparency, fresnel, browse-clarity. |
| `SPHERE_RINGS` / `SPHERE_SEGMENTS` | Tessellation. |

## Module map

| File | Responsibility |
|---|---|
| `src/graph.rs` | The hard-coded "eno" graph: nodes (id, kind, form, pos, ring bits), typed edges, neighbourhood queries. |
| `src/nav.rs` | Id-keyed navigation: trail, transitions, convergence, ambient tint, camera distance. |
| `src/scene.rs` | Per-frame assembly of node / ring / edge instances + labels; picking. |
| `src/render.rs` | wgpu: depth pre-pass, glass, ring, and edge pipelines; capture. |
| `src/hud.rs` | Stats, breadcrumb, billboarded world-space labels. |
| `src/camera.rs`, `src/eversion.rs`, `src/sphere.rs` | Camera, the eased gesture scalar, the sphere mesh. |
| `src/shader.wgsl` | Sphere (glass) + ring + edge programs. |

## Deferred (TODO in code)

Scroll/book leaf forms for individual files (this phase stays at the node-graph
level); any "act"/queue mode; running the actual demo; a config setting for the
default landing point (hardcoded production-first for now).
