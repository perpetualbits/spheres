//! HUD: frame-time stats, breadcrumb, and billboarded world-space labels.
//!
//! Text is rendered with glyphon. Node labels are projected from their world
//! position to screen each frame so names/kinds ride with the spheres.

use std::collections::VecDeque;
use std::time::Instant;

use glam::{Mat4, Vec3, Vec4};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::config;

/// A billboarded world-space label (a node name/kind).
pub struct Label {
    pub text: String,
    pub pos: Vec3,
    pub rgb: [f32; 3],
    pub alpha: f32,
}

/// Rolling frame-time statistics plus the eversion budget monitor.
struct Stats {
    samples: VecDeque<(Instant, f32)>,
    last_ms: f32,
    fps: f32,
    prev_eversion_active: bool,
    live_over: u32,
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

        self.eversion_active = eversion_active;
        if eversion_active && !self.prev_eversion_active {
            self.live_over = 0;
        }
        if eversion_active && frame_ms > config::FRAME_BUDGET_MS {
            self.live_over += 1;
        }
        if !eversion_active && self.prev_eversion_active {
            self.last_over = self.live_over;
        }
        self.prev_eversion_active = eversion_active;
    }

    fn rolling_max(&self) -> f32 {
        self.samples.iter().map(|&(_, ms)| ms).fold(0.0_f32, f32::max)
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
    label_pool: Vec<Buffer>,

    width: u32,
    height: u32,
    stats: Stats,

    depth: usize,
    breadcrumb: String,

    labels: Vec<Label>,
    view_proj: Mat4,
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
            label_pool: Vec::new(),
            width,
            height,
            stats: Stats::new(),
            depth: 0,
            breadcrumb: String::from("overview"),
            labels: Vec::new(),
            view_proj: Mat4::IDENTITY,
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

    pub fn set_nav(&mut self, depth: usize, breadcrumb: String) {
        self.depth = depth;
        self.breadcrumb = breadcrumb;
    }

    /// Provide this frame's world-space labels and the matrix to project them.
    pub fn set_world_labels(&mut self, labels: Vec<Label>, view_proj: Mat4) {
        self.labels = labels;
        self.view_proj = view_proj;
    }

    pub fn record(&mut self, now: Instant, frame_ms: f32, eversion_active: bool) {
        self.stats.record(now, frame_ms, eversion_active);
    }

    pub fn snapshot(&self) -> (f32, f32, f32) {
        (self.stats.last_ms, self.stats.fps, self.stats.rolling_max())
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), glyphon::PrepareError> {
        let s = &self.stats;
        let eversion_line = if s.eversion_active {
            format!("EVERSION (live): {} frames > budget", s.live_over)
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
        let crumb = format!("depth: {}   {}", self.depth, self.breadcrumb);

        let mono = Attrs::new().family(Family::Monospace);
        self.buffer
            .set_text(&mut self.font_system, &text, &mono, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
        self.crumb_buffer
            .set_text(&mut self.font_system, &crumb, &mono, Shaping::Advanced, None);
        self.crumb_buffer
            .shape_until_scroll(&mut self.font_system, false);

        // Grow the label buffer pool to match this frame's labels.
        while self.label_pool.len() < self.labels.len() {
            let mut b = Buffer::new(&mut self.font_system, Metrics::new(15.0, 18.0));
            b.set_size(&mut self.font_system, Some(self.width as f32), Some(self.height as f32));
            self.label_pool.push(b);
        }
        let sans = Attrs::new().family(Family::SansSerif);
        for (i, label) in self.labels.iter().enumerate() {
            self.label_pool[i].set_text(
                &mut self.font_system,
                &label.text,
                &sans,
                Shaping::Advanced,
                None,
            );
            self.label_pool[i].shape_until_scroll(&mut self.font_system, false);
        }

        self.viewport.update(
            queue,
            Resolution {
                width: self.width,
                height: self.height,
            },
        );

        let stats_color = if s.over_budget() {
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

        let mut areas = vec![
            TextArea {
                buffer: &self.buffer,
                left: 12.0,
                top: 12.0,
                scale: 1.0,
                bounds: full_bounds,
                default_color: stats_color,
                custom_glyphs: &[],
            },
            TextArea {
                buffer: &self.crumb_buffer,
                left: 12.0,
                top: (self.height as f32 - 36.0).max(0.0),
                scale: 1.0,
                bounds: full_bounds,
                default_color: Color::rgb(150, 210, 255),
                custom_glyphs: &[],
            },
        ];

        // Project each world-space label to screen.
        for (i, label) in self.labels.iter().enumerate() {
            let clip = self.view_proj * Vec4::new(label.pos.x, label.pos.y, label.pos.z, 1.0);
            if clip.w <= 0.0001 {
                continue; // behind the camera
            }
            let ndc = clip.truncate() / clip.w;
            if ndc.x < -1.3 || ndc.x > 1.3 || ndc.y < -1.3 || ndc.y > 1.3 {
                continue; // well off-screen
            }
            let sx = (ndc.x * 0.5 + 0.5) * self.width as f32;
            let sy = (1.0 - (ndc.y * 0.5 + 0.5)) * self.height as f32;
            let a = (label.alpha.clamp(0.0, 1.0) * 255.0) as u8;
            areas.push(TextArea {
                buffer: &self.label_pool[i],
                left: sx - 28.0,
                top: sy,
                scale: 1.0,
                bounds: full_bounds,
                default_color: Color::rgba(
                    (label.rgb[0] * 255.0) as u8,
                    (label.rgb[1] * 255.0) as u8,
                    (label.rgb[2] * 255.0) as u8,
                    a,
                ),
                custom_glyphs: &[],
            });
        }

        self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        )
    }

    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
    ) -> Result<(), glyphon::RenderError> {
        self.renderer.render(&self.atlas, &self.viewport, pass)
    }
}
