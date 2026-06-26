# Spheres — Design Document

*A 3D-native Wayland compositor and desktop environment whose entire interaction model is the sphere eversion.*

Status: pre-prototype (Phase 0). This document is the north star, not a spec to implement top-to-bottom. It will be wrong in places; the staged plan exists to find out where, cheaply.

---

## 1. The one-sentence idea

A program, a file, a printer, a network — each is a sphere hanging in space. Activating one everts it: the sphere turns inside-out and envelops you, and now you are inside its world. Inside, its contents are themselves spheres, enterable the same way. One geometric gesture — the eversion — is the desktop's single interaction verb, composing to any depth.

## 2. Why this is worth building

Most 3D desktops are a 2D desktop tilted, with the third dimension carrying no meaning. Here the dimensionality is load-bearing: **enclosure means context**, the surface you are inside of *is* the application you are within, and **crossing the surface is switching context**. The metaphor is recursive and uniform — nested worlds under a single law — which is the opposite of today's desktop, a pile of incommensurable metaphors (windows, taskbars, modals, tabs, trays, toasts). That conceptual unity is the reason a from-scratch compositor earns its existence rather than being a solution in search of a problem. A 3D compositor with a flat desktop on it has no point; a 3D compositor whose whole interaction model is spatial does.

## 3. Architecture: three ambitions, one decision

Multithreaded, extension-isolated, and safe are not three projects. They are one architectural choice seen from three sides. GNOME is single-threaded *because* foreign code (extensions) touches the scene graph directly, and a shared mutable scene graph across threads is the GObject-affinity nightmare. **Spheres designs foreign code out of the scene graph entirely.** The compositor core is 100% our own trusted Rust, which means:

- it can be threaded freely, because nothing foreign has thread-affinity constraints on the scene graph;
- extensions are isolated by construction, because they were never in the core to begin with;
- the core is memory-safe internally (Rust), not merely defended at its boundary.

The thing that makes this hard for GNOME is the thing we delete in line one.

### 3.1 Safety model: microkernel, not monolith

The target is **fault containment**, like a microkernel (seL4, QNX), not the monolithic-kernel model where a bad module panics everything (which is precisely the GNOME failure mode). A misbehaving extension can fail to answer its own mailbox and get killed; it cannot stall the compositor, because it is neither on the compositor's thread nor in its process/sandbox. Memory-safe Rust core + fault-isolated extensions + capability-gated host API = a safety floor neither a C microkernel nor a monolithic kernel has.

### 3.2 The extension keystone: privileged Wayland client + WASM brain

This is the most important design decision in the project.

A compositor already owns a battle-tested protocol for "an untrusted thing contributes visual content without touching my internals": the Wayland protocol itself. So **an extension is a privileged Wayland client.**

- **Visual half:** the extension renders into a surface and hands it to us as a dmabuf, exactly like any application, through a layer-shell-style protocol. We do not invent an extension drawing API — *client isolation IS extension isolation.* A hung extension can no more freeze the compositor than Firefox can.
- **Logic half:** sandboxed WASM under Wasmtime. It has no syscalls; it can only call host functions we import. The only I/O-shaped imports we provide are async-yielding, so **a blocking freeze is unrepresentable, not merely discouraged.** Epoch interruption / fuel metering means a `loop {}` or runaway computation traps mid-execution — real preemption.

Consequences that fall out for free:
- **Language-agnostic.** Anything that compiles to WASM: Rust, C, or JavaScript-inside-WASM (QuickJS via Javy) so we need not orphan JS authors.
- **Preemption for free** — not by interrupting in-process JS (impossible safely), but because killing a sandbox/process is trivial.
- **The correctness burden moves to the right place** — async-only must be enforced host-side too, but that is a handful of trusted host-API authors, audited once, instead of thousands of extension authors, audited never.

The cost, stated honestly: we lose monkey-patching. An isolated extension can only do what the host API exposes, so that API must be designed and versioned deliberately. We accept this; it is the price of the blast-radius guarantee, and it is the same coin as extensibility viewed from the other side.

### 3.3 Realtime discipline: three guarantee classes

The interactive loop is a hard-realtime task with a deadline; nothing may make it miss. "Never block the compositor" was the special case. The general law is **bounded, admitted, degradable response.** Guarantee on *response, not completion*: we never promise that listing a million files finishes instantly — we promise the acknowledgment lands within deadline and that bounded increments (a page) complete in bounded time. The loop is realtime; the work is best-effort; the interface between them is bounded chunks.

Three honest classes:

- **CPU-hard** — input acknowledgment, scheduling, admission control. Genuinely hard; the CPU preempts cleanly. Free on a `PREEMPT_RT` kernel (mainline since 6.12, though distros may still need an RT-built kernel).
- **Audio-hard** — copy the PipeWire model wholesale: high-priority callback, hard deadline = buffer period, xrun *reported* not swallowed. PipeWire already runs a realtime graph for audio *and* video; part of the substrate exists.
- **GPU-firm** — GPUs are throughput-optimized with coarse preemption and execution-time variability; a deadline miss means *degrade and report*, not a hard guarantee. This is acknowledged, not papered over.

**Admission control** is a schedulability test: before admitting the 11th video, check whether existing deadlines still hold; if not, refuse or degrade *explicitly*. The opposite of today's silent jank. Precedents to imitate: the kernel's `SCHED_DEADLINE` (EDF with built-in admission), `sched_rt_runtime_us` (reserves a slice for non-RT so a runaway can't starve the system), and PipeWire's `rt.time.soft/hard` (kernel kills an RT thread that burns CPU without yielding — the same fuel-budget idea as the WASM extension layer).

**Backpressure has a visual home.** In a flat desktop, "out of budget" is an ugly toast. In Spheres, a resource-starved world *is a sphere* — it can dim, destabilize, refuse to fully evert, visibly throttle. Admission control gets a native visual language, and the recursion means each nested world can carry its own budget.

## 4. The stack

| Concern | Choice | Notes |
|---|---|---|
| Wayland plumbing, DRM/KMS, libinput, dmabuf import | **Smithay** | Rust. `anvil` is the reference compositor / hello-world. |
| 3D renderer | **wgpu** | Multithreaded command encoding. See the seam in §6. |
| Extension runtime | **Wasmtime** | Component model + WIT, async support, epoch/fuel preemption. |
| Session / device access | **libseat / seatd** | No root. |
| Flat 2D surfaces *inside* spheres | **iced / libcosmic** | The inside of a sphere is a hostile place to read a PDF; focused work happens on flat surfaces facing the viewer within the spherical world. iced renders those, composited as textures into the 3D scene. |
| Audio + video RT substrate | **PipeWire** | Graph engine; the model to generalize. |
| Kernel | **PREEMPT_RT** (6.12+) | For the CPU-hard class. |

**COSMIC is a reference, not a fork base.** It is now stable (Pop!_OS 24.04 LTS, Dec 2025; COSMIC 1.1, June 2026) and a trustworthy, actively-patched *map of the swamps* — go read how `cosmic-comp` handles screencopy, portals, XWayland, session/seat, hotplug. Its applet model (privileged layer-shell clients, a hung applet can't freeze the compositor) is a working existence proof of our extension architecture. We do not fork it because its core composites windows as 2D rectangles, which is the exact thing Spheres replaces.

## 5. Staged plan

Each phase is independently demoable and de-risks exactly one thesis. Resist building the real desktop; build the spine that proves the architecture.

- **Phase 0 — The single everting sphere.** Pure wgpu, no compositor. One sphere hanging in space; click to evert and envelop; gesture to reverse. **Frame-time readout on screen from day one.** Answers the only two questions that gate everything: *does the gesture feel like magic or nausea*, and *can we hold the frame deadline through the worst frame in the system* (the eversion is that worst frame — see §6). This is the current task.
- **Phase 1 — A window on screen.** Build `anvil` on a bare TTY; get a client rendering. We are now a (trivial) display server.
- **Phase 2 — The renderer is ours.** Replace anvil's rendering with wgpu; composite client windows as textured quads via zero-copy dmabuf import. **This is the seam to spike (§6).**
- **Phase 3 — 3D-native.** Windows become quads in a real scene with a camera. New problem: ray-picking against rotated quads instead of rectangle hit-testing.
- **Phase 4 — Thread it.** Render / input / scene-update threads. Normal concurrency for us, because nothing foreign touches the scene graph.
- **Phase 5 — The extension engine.** A tiny WIT host API ("subscribe to window-focus", "place a surface here"); one trivial isolated extension; prove `loop {}` inside it cannot freeze the compositor.
- **Beyond.** The long tail of *being a real desktop*: xdg-shell, layer-shell, clipboard, drag-and-drop, screen capture via xdg-desktop-portal, fractional scaling, VRR/HDR, suspend/resume, hotplug, XWayland. This is where the years live. Decide about the daily-driver desktop only once the theses are proven.

## 6. Known seams and risks

- **wgpu-on-Smithay bridge (the first real risk).** wgpu is "too abstract for internal compositor work" — it does not expose all the Vulkan extensions compositors need. The documented resolution matches our plan: Smithay does the dmabuf import (the privileged, extension-heavy part), wgpu renders on top. Some compositor-specific bits may still need raw Vulkan. Spike this in Phase 2 with the smallest possible "import one client buffer, sample it as a wgpu texture, composite it."
- **The eversion is the worst-case frame.** Full-scene geometry + moving camera + whole viewport in motion = the most expensive frame *and* the one where a dropped frame hurts most (nausea). It sets the **resource floor**: budget the whole realtime system around the eversion, not the idle desktop. This is why the frame-time readout exists from Phase 0.
- **3D-native fights damage tracking.** 2D compositors redraw only changed rectangles; a 3D scene with a moving camera tends toward full-frame redraws → fans, battery. Claw back partial-damage optimization once the prototype works; it is genuinely hard in 3D.
- **The vestibular tax.** An interface triggered hundreds of times a day cannot make you slightly queasy each time. Mitigations: keep the eversion **fast and decisive, not slow and swimmy** (this aligns with the realtime discipline — fewer worst-case frames to guarantee — so aesthetics and engineering agree against the indulgent instinct); stable reference frame during transition; user-dialable intensity down to near-cut.
- **Legibility — everything is a featureless sphere.** A sphere is maximally symmetric and minimally informative. All identity migrates onto surface treatment (texture, material, what orbits it, how it moves) and spatial arrangement (printers live *here*, files cluster *there*). Prototype "can I tell forty of these apart at a glance" early.
- **Navigation — getting lost in depth.** Design the "surface to top level" escape gesture *before* the "go in" gesture; the way out must be more reliable than the way in. Spatial memory (things stay where you put them) and a visible nesting-depth trail (breadcrumb spheres receding behind you).
- **The dev machine is hard mode.** The hybrid Intel-display + NVIDIA-dGPU laptop (where this whole investigation started) is one of the nastiest compositor-dev cases: PRIME buffer-sharing / cross-GPU dmabuf import is exactly where compositors break. **Develop against a single-GPU target first; treat the hybrid path as its own later phase.**

## 7. Connection to aerie

`aerie` (the latency scope built while diagnosing the freeze that started all this) is the natural home for a **main-loop / frame stall detector that names the offending handler** — the per-frame watchdog GNOME doesn't ship for itself. Spheres' realtime layer and aerie's instrumentation are the same instinct: make the deadline miss *visible and attributable* rather than something you diagnose for a week in 0.5-second bites.

## 8. Guiding principle

The mind that always descends to the next layer can rewrite a working desktop into a from-scratch 3D compositor in five sentences, and the descent feels identical whether it leads somewhere real or not. The discipline that rides alongside it is: **ship the one sphere.** The deep principle is worthless until one running sphere agrees with it. Build the cheap, falsifying experiment first — always.
