//! Spheres — Phase 0.5.
//!
//! A standalone wgpu/winit prototype of the recursive sphere desktop: glassy
//! spheres hanging in space, each previewing the world inside it; click one to
//! evert into it and arrive in a new space of child spheres; Esc to surface
//! back out one level. NOT a compositor — it exists to test whether the
//! recursion feels coherent and the world stays legible and navigable, while
//! keeping a frame deadline through the (now populated) worst-case eversion.
//!
//! Controls:
//!   left click            -> evert into the sphere under the cursor
//!   right click / Esc     -> surface out one level (reliable from any state)
//!   Q / window close      -> quit

mod camera;
mod config;
mod eversion;
mod hud;
mod nav;
mod render;
mod scene;
mod sphere;
mod world;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use camera::Camera;
use hud::Hud;
use nav::Nav;
use render::Renderer;

/// Everything that exists only once the window (and GPU surface) is alive.
struct State {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    nav: Nav,
    hud: Hud,

    width: u32,
    height: u32,
    /// Cursor in normalised device coords ([-1,1], y up); defaults to centre.
    cursor_ndc: (f32, f32),

    start: Instant,
    last_frame: Instant,

    /// When set (via the `SPHERES_AUTODEMO` env var), dives and surfaces fire
    /// themselves. Handy for headless frame-time capture of the populated
    /// eversion without a mouse.
    auto_demo: bool,
    next_action: f32,

    /// When set (via `SPHERES_CAPTURE=path`), render a few scripted frames to
    /// `path.*.ppm` and exit. A compositor-independent way to actually see what
    /// the GPU drew.
    capture_path: Option<String>,

    /// `SPHERES_PERFLOG=1`: periodically log frame stats to stderr.
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

    /// Self-driving demo: dive deeper until AUTODEMO_MAX_DEPTH, then surface
    /// all the way back, repeatedly — only acting while resting.
    fn drive_auto_demo(&mut self, elapsed: f32) {
        if !self.auto_demo || elapsed < self.next_action || self.nav.is_everting() {
            return;
        }
        if self.nav.depth() < config::AUTODEMO_MAX_DEPTH {
            let target = (self.nav.depth() * 3 + 1) % config::SPHERES_PER_LEVEL;
            self.nav.evert_in(target);
        } else {
            self.nav.surface_out();
        }
        self.next_action = elapsed + 1.4;
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
        self.hud.set_nav(self.nav.depth(), self.nav.breadcrumb());

        // Optional headless perf log (SPHERES_PERFLOG=1) — the same numbers the
        // on-screen HUD shows, for measuring without reading the window.
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

        let frame = scene::build(&self.nav, &self.camera, time, self.cursor_ndc);

        self.renderer.render(
            &self.camera,
            self.nav.eased(),
            time,
            &frame.opaque,
            &frame.glass,
            &mut self.hud,
            false,
        );
    }

    /// Render a few scripted frames to PPM files and exit. Compositor-
    /// independent visual verification of what the GPU actually drew.
    fn run_capture(&mut self) -> ! {
        let base = self.capture_path.clone().unwrap();

        // 1) Resting at the root: the populated, glassy, legible scene.
        self.nav = Nav::new();
        self.shoot(&format!("{base}.root.ppm"), 2.0);

        // 2) Fully inside child 3: a fresh world of its own children.
        self.nav = Nav::new();
        self.nav.evert_in(3);
        self.nav.update(10.0); // run to completion (commits the descent)
        self.shoot(&format!("{base}.inside.ppm"), 2.0);

        // 3) Mid-eversion into child 2: the worst-case frame, caught halfway.
        self.nav = Nav::new();
        self.nav.evert_in(2);
        self.nav.update(0.5 * config::EVERSION_DURATION_MS / 1000.0);
        self.shoot(&format!("{base}.evert.ppm"), 2.0);

        std::process::exit(0);
    }

    fn shoot(&mut self, path: &str, time: f32) {
        self.hud.record(Instant::now(), 8.0, self.nav.is_everting());
        self.hud.set_nav(self.nav.depth(), self.nav.breadcrumb());
        let frame = scene::build(&self.nav, &self.camera, time, (0.0, 0.0));
        if let Some(img) = self.renderer.render(
            &self.camera,
            self.nav.eased(),
            time,
            &frame.opaque,
            &frame.glass,
            &mut self.hud,
            true,
        ) {
            match write_ppm(path, &img) {
                Ok(()) => log::info!("captured {} ({}x{})", path, img.width, img.height),
                Err(e) => log::error!("capture write {path} failed: {e}"),
            }
        } else {
            log::error!("capture {path}: no frame acquired");
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
            .with_title("Spheres — Phase 0.5")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );
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
                    // Enter the sphere under the cursor (if any).
                    if let Some(i) =
                        scene::pick(&state.nav, &state.camera, state.cursor_ndc.0, state.cursor_ndc.1)
                    {
                        state.nav.evert_in(i);
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
        // Drive a continuous animation/measurement loop.
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
