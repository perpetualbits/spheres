//! Frame-time HUD.
//!
//! Always-on instrumentation, required from day one: current frame time and
//! FPS, the rolling MAX frame time over the last few seconds, a red highlight
//! whenever a frame blows the budget, and — the thing we actually care about —
//! a persistent count of how many frames blew the budget during the *last
//! eversion*, since the eversion is our worst-case frame.
//!
//! Text is rendered with glyphon (cosmic-text + a wgpu glyph atlas).

use std::collections::VecDeque;
use std::time::Instant;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::config;

/// Rolling frame-time statistics plus the eversion budget monitor.
struct Stats {
    /// (timestamp, frame_ms) within the rolling window.
    samples: VecDeque<(Instant, f32)>,
    last_ms: f32,
    fps: f32,

    // Eversion worst-case monitoring.
    prev_eversion_active: bool,
    /// Over-budget frames in the eversion currently in flight.
    live_over: u32,
    /// Over-budget frames from the most recently completed eversion.
    last_over: u32,
    eversion_active: bool,
}

impl Stats {
    fn new() -> Self {
        Stats {
            samples: VecDeque::new(),
            last_ms: 0.0,
            fps: 0.0,
            prev_eversion_active: false,
            live_over: 0,
            last_over: 0,
            eversion_active: false,
        }
    }

    fn record(&mut self, now: Instant, frame_ms: f32, eversion_active: bool) {
        self.last_ms = frame_ms;
        // Smooth FPS a little so the readout is legible.
        let inst_fps = if frame_ms > 0.0 { 1000.0 / frame_ms } else { 0.0 };
        self.fps = if self.fps == 0.0 {
            inst_fps
        } else {
            self.fps * 0.9 + inst_fps * 0.1
        };

        self.samples.push_back((now, frame_ms));
        let window = std::time::Duration::from_secs_f32(config::ROLLING_WINDOW_SECS);
        while let Some(&(t, _)) = self.samples.front() {
            if now.duration_since(t) > window {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // Eversion budget monitor.
        self.eversion_active = eversion_active;
        if eversion_active && !self.prev_eversion_active {
            // A fresh eversion just started.
            self.live_over = 0;
        }
        if eversion_active && frame_ms > config::FRAME_BUDGET_MS {
            self.live_over += 1;
        }
        if !eversion_active && self.prev_eversion_active {
            // The eversion just finished; freeze the count.
            self.last_over = self.live_over;
        }
        self.prev_eversion_active = eversion_active;
    }

    fn rolling_max(&self) -> f32 {
        self.samples
            .iter()
            .map(|&(_, ms)| ms)
            .fold(0.0_f32, f32::max)
    }

    fn over_budget(&self) -> bool {
        self.last_ms > config::FRAME_BUDGET_MS
    }
}

pub struct Hud {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    crumb_buffer: Buffer,

    width: u32,
    height: u32,
    stats: Stats,

    /// Current navigation depth and breadcrumb trail, set each frame.
    depth: usize,
    breadcrumb: String,
}

impl Hud {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let mut buffer = Buffer::new(&mut font_system, Metrics::new(18.0, 22.0));
        buffer.set_size(&mut font_system, Some(width as f32), Some(height as f32));
        let mut crumb_buffer = Buffer::new(&mut font_system, Metrics::new(20.0, 24.0));
        crumb_buffer.set_size(&mut font_system, Some(width as f32), Some(height as f32));

        Hud {
            font_system,
            swash_cache,
            viewport,
            atlas,
            renderer,
            buffer,
            crumb_buffer,
            width,
            height,
            stats: Stats::new(),
            depth: 0,
            breadcrumb: String::from("Root"),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.buffer
            .set_size(&mut self.font_system, Some(width as f32), Some(height as f32));
        self.crumb_buffer
            .set_size(&mut self.font_system, Some(width as f32), Some(height as f32));
    }

    /// Set the navigation depth + breadcrumb trail for this frame's readout.
    pub fn set_nav(&mut self, depth: usize, breadcrumb: String) {
        self.depth = depth;
        self.breadcrumb = breadcrumb;
    }

    /// Record this frame's timing. `eversion_active` is true while the gesture
    /// is mid-flight.
    pub fn record(&mut self, now: Instant, frame_ms: f32, eversion_active: bool) {
        self.stats.record(now, frame_ms, eversion_active);
    }

    /// (last_ms, fps, rolling_max_ms) — for optional headless perf logging.
    pub fn snapshot(&self) -> (f32, f32, f32) {
        (self.stats.last_ms, self.stats.fps, self.stats.rolling_max())
    }

    /// Build the HUD text and upload glyphs. Call once per frame, before the
    /// render pass.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), glyphon::PrepareError> {
        let s = &self.stats;
        let eversion_line = if s.eversion_active {
            format!(
                "EVERSION (live): {} frames > budget",
                s.live_over
            )
        } else {
            format!("eversion (last): {} frames > budget", s.last_over)
        };

        let text = format!(
            "frame: {:.2} ms   fps: {:.0}\nmax ({:.0}s): {:.2} ms   budget: {:.1} ms\n{}",
            s.last_ms,
            s.fps,
            config::ROLLING_WINDOW_SECS,
            s.rolling_max(),
            config::FRAME_BUDGET_MS,
            eversion_line,
        );

        let attrs = Attrs::new().family(Family::Monospace);
        self.buffer
            .set_text(&mut self.font_system, &text, &attrs, Shaping::Advanced, None);
        self.buffer
            .shape_until_scroll(&mut self.font_system, false);

        // Breadcrumb / depth trail — navigation must always be visible.
        let crumb = format!("depth: {}   {}", self.depth, self.breadcrumb);
        self.crumb_buffer
            .set_text(&mut self.font_system, &crumb, &attrs, Shaping::Advanced, None);
        self.crumb_buffer
            .shape_until_scroll(&mut self.font_system, false);

        self.viewport.update(
            queue,
            Resolution {
                width: self.width,
                height: self.height,
            },
        );

        // Red when the last frame blew the budget, otherwise a calm green.
        let color = if s.over_budget() {
            Color::rgb(255, 70, 70)
        } else {
            Color::rgb(120, 230, 140)
        };

        let full_bounds = TextBounds {
            left: 0,
            top: 0,
            right: self.width as i32,
            bottom: self.height as i32,
        };

        let stats_area = TextArea {
            buffer: &self.buffer,
            left: 12.0,
            top: 12.0,
            scale: 1.0,
            bounds: full_bounds,
            default_color: color,
            custom_glyphs: &[],
        };

        let crumb_area = TextArea {
            buffer: &self.crumb_buffer,
            left: 12.0,
            top: (self.height as f32 - 36.0).max(0.0),
            scale: 1.0,
            bounds: full_bounds,
            default_color: Color::rgb(150, 210, 255),
            custom_glyphs: &[],
        };

        self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            [stats_area, crumb_area],
            &mut self.swash_cache,
        )
    }

    /// Draw the prepared text into an existing render pass.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) -> Result<(), glyphon::RenderError> {
        self.renderer.render(&self.atlas, &self.viewport, pass)
    }
}
