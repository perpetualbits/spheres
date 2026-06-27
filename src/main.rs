//! Spheres — Phase 0.6.
//!
//! A standalone wgpu/winit prototype of the spatial/relational interface, now
//! over a hand-authored graph of the "eno" project. NOT a compositor. The goal
//! is "to BE somewhere": every node is a glassy sphere (a container you evert
//! into), carrying a Saturn ring that reads out its data; nodes are linked by
//! glowing edges so the global structure — core's centrality, the hidden
//! cross-couplings, a person spanning libraries and tools — is visible at a
//! glance. Nodes are keyed by stable id, so reaching one two ways converges on
//! the same world.
//!
//! Controls:
//!   left click            -> evert into the node under the cursor
//!   right click / Esc     -> surface out one level (reliable from any state)
//!   Q / window close      -> quit
//!
//! TODO (deferred): scroll/book leaf forms for individual files; an "act" mode;
//! running the demo. CONFIG: default landing point is hardcoded production-first.

mod camera;
mod config;
mod eversion;
mod graph;
mod hud;
mod nav;
mod render;
mod scene;
mod sphere;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use camera::Camera;
use graph::Graph;
use hud::Hud;
use nav::Nav;
use render::Renderer;

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    graph: Graph,
    nav: Nav,
    hud: Hud,

    width: u32,
    height: u32,
    cursor_ndc: (f32, f32),

    start: Instant,
    last_frame: Instant,

    auto_demo: bool,
    next_action: f32,

    capture_path: Option<String>,
    perf_log: bool,
    frame_count: u64,
}

impl State {
    fn new(window: Arc<Window>) -> Self {
        let renderer = Renderer::new(window.clone());
        let (w, h) = renderer.size();
        let camera = Camera::new(w as f32 / h as f32);
        let hud = Hud::new(renderer.device(), renderer.queue(), renderer.format(), w, h);

        let now = Instant::now();
        State {
            window,
            renderer,
            camera,
            graph: Graph::eno(),
            // CONFIG: default landing point. Hardcoded production-first means we
            // begin in the global overview with production available; a future
            // config setting would let this start inside a chosen node.
            nav: Nav::new(),
            hud,
            width: w,
            height: h,
            cursor_ndc: (0.0, 0.0),
            start: now,
            last_frame: now,
            auto_demo: std::env::var_os("SPHERES_AUTODEMO").is_some(),
            next_action: 1.0,
            capture_path: std::env::var("SPHERES_CAPTURE").ok(),
            perf_log: std::env::var_os("SPHERES_PERFLOG").is_some(),
            frame_count: 0,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.renderer.resize(width, height);
        self.hud.resize(width, height);
        if height > 0 {
            self.camera.aspect = width as f32 / height as f32;
        }
    }

    fn set_cursor(&mut self, pos: PhysicalPosition<f64>) {
        let x = (pos.x as f32 / self.width as f32) * 2.0 - 1.0;
        let y = 1.0 - (pos.y as f32 / self.height as f32) * 2.0;
        self.cursor_ndc = (x, y);
    }

    /// Self-driving demo: overview → production → core, then surface back, on a
    /// timer. Acts only while resting.
    fn drive_auto_demo(&mut self, elapsed: f32) {
        if !self.auto_demo || elapsed < self.next_action || self.nav.is_everting() {
            return;
        }
        match self.nav.depth() {
            0 => self.nav.evert_in(self.graph.production()),
            1 => self.nav.evert_in(0), // core is node 0
            _ => self.nav.surface_out(),
        }
        self.next_action = elapsed + 1.5;
    }

    fn frame(&mut self) {
        if self.capture_path.is_some() {
            self.run_capture();
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        let time = now.duration_since(self.start).as_secs_f32();

        self.drive_auto_demo(time);
        self.nav.update(dt);

        self.hud.record(now, dt * 1000.0, self.nav.is_everting());
        self.hud.set_nav(self.nav.depth(), self.nav.breadcrumb(&self.graph));

        self.frame_count += 1;
        if self.perf_log && self.frame_count % 120 == 0 {
            let (last, fps, max) = self.hud.snapshot();
            log::info!(
                "perf: {:.2} ms  {:.0} fps  max(3s) {:.2} ms  depth {}",
                last,
                fps,
                max,
                self.nav.depth()
            );
        }

        let eye_z = self.nav.eye_distance();
        let view_proj = self.camera.view_proj(eye_z);
        let frame = scene::build(&self.graph, &self.nav, &self.camera, time, self.cursor_ndc);
        self.hud.set_world_labels(frame.labels, view_proj);

        self.renderer.render(
            &self.camera,
            eye_z,
            time,
            frame.clear,
            &frame.nodes,
            &frame.rings,
            &frame.edges,
            &mut self.hud,
            false,
        );
    }

    /// Render scripted frames to PPM and exit. Notably captures `core` reached
    /// two ways to demonstrate id-keyed convergence.
    fn run_capture(&mut self) -> ! {
        let base = self.capture_path.clone().unwrap();
        let core = 0;
        let production = self.graph.production();
        let io = self.graph.nodes.iter().position(|n| n.name == "io").unwrap();

        // 1) The global overview — the structure.
        self.nav = Nav::new();
        self.shoot(&format!("{base}.overview.ppm"));

        // 2) Inside production.
        self.nav = Nav::new();
        self.nav.evert_in(production);
        self.nav.update(10.0);
        self.shoot(&format!("{base}.production.ppm"));

        // 3) core reached via production.
        self.nav.evert_in(core);
        self.nav.update(10.0);
        self.shoot(&format!("{base}.core-via-production.ppm"));

        // 4) core reached via io — MUST be the same world (convergence).
        self.nav = Nav::new();
        self.nav.evert_in(io);
        self.nav.update(10.0);
        self.nav.evert_in(core);
        self.nav.update(10.0);
        self.shoot(&format!("{base}.core-via-io.ppm"));

        // 5) Mid-eversion into core.
        self.nav = Nav::new();
        self.nav.evert_in(core);
        self.nav.update(0.5 * config::EVERSION_DURATION_MS / 1000.0);
        self.shoot(&format!("{base}.evert.ppm"));

        std::process::exit(0);
    }

    fn shoot(&mut self, path: &str) {
        let time = 2.0;
        self.hud.record(Instant::now(), 8.0, self.nav.is_everting());
        self.hud.set_nav(self.nav.depth(), self.nav.breadcrumb(&self.graph));
        let eye_z = self.nav.eye_distance();
        let view_proj = self.camera.view_proj(eye_z);
        let frame = scene::build(&self.graph, &self.nav, &self.camera, time, (0.0, 0.0));
        self.hud.set_world_labels(frame.labels, view_proj);
        if let Some(img) = self.renderer.render(
            &self.camera,
            eye_z,
            time,
            frame.clear,
            &frame.nodes,
            &frame.rings,
            &frame.edges,
            &mut self.hud,
            true,
        ) {
            match write_ppm(path, &img) {
                Ok(()) => log::info!("captured {} ({}x{})", path, img.width, img.height),
                Err(e) => log::error!("capture write {path} failed: {e}"),
            }
        }
    }
}

fn write_ppm(path: &str, img: &render::Captured) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P6\n{} {}\n255\n", img.width, img.height)?;
    f.write_all(&img.rgb)?;
    f.flush()
}

#[derive(Default)]
struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Spheres — Phase 0.6")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.state = Some(State::new(window));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => state.set_cursor(position),

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => match button {
                MouseButton::Left => {
                    if let Some(id) = scene::pick(
                        &state.graph,
                        &state.nav,
                        &state.camera,
                        state.cursor_ndc.0,
                        state.cursor_ndc.1,
                    ) {
                        state.nav.evert_in(id);
                    }
                }
                MouseButton::Right => state.nav.surface_out(),
                _ => {}
            },

            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => state.nav.surface_out(),
                    Key::Character(ref c) if c.eq_ignore_ascii_case("q") => event_loop.exit(),
                    _ => {}
                }
            }

            WindowEvent::RedrawRequested => state.frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop");
}
