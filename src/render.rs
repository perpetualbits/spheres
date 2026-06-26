//! wgpu setup and the per-frame draw.
//!
//! Owns the surface, device/queue, the shared sphere mesh, the two render
//! pipelines (opaque cores/moons, then translucent glass shells), the depth
//! texture, and the global uniform block. Instance data is built each frame by
//! `scene` and uploaded into growable instance buffers. The HUD is drawn last,
//! in a depth-less pass.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::Camera;
use crate::config;
use crate::hud::Hud;
use crate::sphere::{Mesh, Vertex};

/// Per-instance data. Mirrors the `@location(1..=3)` inputs in `shader.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    center_radius: [f32; 4], // xyz center, w radius
    tint_alpha: [f32; 4],    // rgb tint, a alpha
    params: [f32; 4],        // evert, focus, pattern, spin
}

impl Instance {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        center: Vec3,
        radius: f32,
        tint: Vec3,
        alpha: f32,
        evert: f32,
        focus: f32,
        pattern: f32,
        spin: f32,
    ) -> Self {
        Instance {
            center_radius: [center.x, center.y, center.z, radius],
            tint_alpha: [tint.x, tint.y, tint.z, alpha],
            params: [evert, focus, pattern, spin],
        }
    }

    /// A plain opaque sphere (preview core or moon).
    pub fn solid(center: Vec3, radius: f32, tint: Vec3) -> Self {
        Instance::new(center, radius, tint, 1.0, 0.0, 0.0, 0.0, 0.0)
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            self.center_radius[0],
            self.center_radius[1],
            self.center_radius[2],
        )
    }

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
            1 => Float32x4, // center_radius
            2 => Float32x4, // tint_alpha
            3 => Float32x4, // params
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        }
    }
}

/// Global uniform block. Mirrors `Globals` in `shader.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4], // xyz eye, w time
    light_dir: [f32; 4],
    fold: [f32; 4],   // amp, freq, travel, _
    glass: [f32; 4],  // opacity_center, opacity_edge, fresnel_power, refraction
    glass2: [f32; 4], // focus_clarity, inner_boost, _, _
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A frame read back from the GPU (debug capture only). RGB, row-tight.
pub struct Captured {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,

    opaque_pipeline: wgpu::RenderPipeline,
    glass_pipeline: wgpu::RenderPipeline,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,

    opaque_instances: wgpu::Buffer,
    opaque_cap: usize,
    glass_instances: wgpu::Buffer,
    glass_cap: usize,

    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    depth_view: wgpu::TextureView,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter");

        log::info!("adapter: {:?}", adapter.get_info());

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("spheres device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            // COPY_SRC so the debug capture path can read back a frame.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width,
            height,
            // No vsync, so the measured frame time reflects real work — the
            // whole point of the HUD is to watch the populated eversion spike.
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // --- Shared mesh ---
        let mesh = Mesh::uv_sphere(config::SPHERE_RINGS, config::SPHERE_SEGMENTS);
        log::info!(
            "sphere mesh: {} vertices, {} triangles",
            mesh.vertices.len(),
            mesh.indices.len() / 3
        );
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let index_count = mesh.indices.len() as u32;

        // --- Instance buffers (grow on demand) ---
        let opaque_cap = 512;
        let glass_cap = 256;
        let opaque_instances = new_instance_buffer(&device, opaque_cap, "opaque instances");
        let glass_instances = new_instance_buffer(&device, glass_cap, "glass instances");

        // --- Uniforms ---
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // --- Pipelines ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sphere shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sphere pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let buffers = [Vertex::layout(), Instance::layout()];

        let opaque_pipeline = make_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            &buffers,
            format,
            "fs_solid",
            wgpu::BlendState::REPLACE,
            true, // depth write
            "opaque pipeline",
        );
        let glass_pipeline = make_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            &buffers,
            format,
            "fs_glass",
            wgpu::BlendState::ALPHA_BLENDING,
            false, // no depth write — let shells blend
            "glass pipeline",
        );

        let depth_view = create_depth_view(&device, width, height);

        Renderer {
            surface,
            device,
            queue,
            surface_config,
            format,
            opaque_pipeline,
            glass_pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            opaque_instances,
            opaque_cap,
            glass_instances,
            glass_cap,
            uniform_buffer,
            bind_group,
            depth_view,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    pub fn size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.depth_view = create_depth_view(&self.device, width, height);
    }

    fn build_globals(&self, camera: &Camera, eased_t: f32, time: f32) -> Globals {
        let view_proj = camera.view_proj(eased_t);
        let eye = camera.eye(eased_t);
        let light = Vec3::from(config::LIGHT_DIR).normalize();

        Globals {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [eye.x, eye.y, eye.z, time],
            light_dir: [light.x, light.y, light.z, 0.0],
            fold: [config::FOLD_AMP, config::FOLD_FREQ, config::FOLD_TRAVEL, 0.0],
            glass: [
                config::GLASS_OPACITY_CENTER,
                config::GLASS_OPACITY_EDGE,
                config::GLASS_FRESNEL_POWER,
                config::GLASS_REFRACTION,
            ],
            glass2: [
                config::GLASS_FOCUS_CLARITY,
                config::GLASS_INNER_BOOST,
                0.0,
                0.0,
            ],
        }
    }

    /// Draw one frame: opaque cores/moons, then glass shells, then the HUD.
    /// When `capture` is true, the rendered frame is read back and returned
    /// (debug only — see `SPHERES_CAPTURE`).
    pub fn render(
        &mut self,
        camera: &Camera,
        eased_t: f32,
        time: f32,
        opaque: &[Instance],
        glass: &[Instance],
        hud: &mut Hud,
        capture: bool,
    ) -> Option<Captured> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.surface_config);
                return None;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::warn!("surface validation error; skipping frame");
                return None;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let globals = self.build_globals(camera, eased_t, time);
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&globals));

        // Upload instance data, growing buffers if needed.
        if opaque.len() > self.opaque_cap {
            self.opaque_cap = opaque.len().next_power_of_two();
            self.opaque_instances =
                new_instance_buffer(&self.device, self.opaque_cap, "opaque instances");
        }
        if glass.len() > self.glass_cap {
            self.glass_cap = glass.len().next_power_of_two();
            self.glass_instances =
                new_instance_buffer(&self.device, self.glass_cap, "glass instances");
        }
        if !opaque.is_empty() {
            self.queue
                .write_buffer(&self.opaque_instances, 0, bytemuck::cast_slice(opaque));
        }
        if !glass.is_empty() {
            self.queue
                .write_buffer(&self.glass_instances, 0, bytemuck::cast_slice(glass));
        }

        hud.prepare(&self.device, &self.queue).expect("hud prepare");

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        // Pass 1: opaque cores/moons, clearing colour + depth.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: config::CLEAR_COLOR[0],
                            g: config::CLEAR_COLOR[1],
                            b: config::CLEAR_COLOR[2],
                            a: config::CLEAR_COLOR[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !opaque.is_empty() {
                pass.set_pipeline(&self.opaque_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.opaque_instances.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.index_count, 0, 0..opaque.len() as u32);
            }
        }

        // Pass 2: glass shells, blended, depth-tested but not depth-written.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glass pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !glass.is_empty() {
                pass.set_pipeline(&self.glass_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.glass_instances.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.index_count, 0, 0..glass.len() as u32);
            }
        }

        // Pass 3: HUD, no depth, over everything.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            hud.render(&mut pass).expect("hud render");
        }

        let captured = if capture {
            Some(self.read_back(&mut encoder, &frame.texture))
        } else {
            None
        };

        self.queue.submit(std::iter::once(encoder.finish()));

        let captured = captured.map(|(buffer, padded, w, h)| {
            // Map and unpad the staging buffer into a row-tight RGB image.
            buffer.map_async(wgpu::MapMode::Read, .., |_| {});
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
            let data = buffer.slice(..).get_mapped_range();
            let is_bgra = matches!(
                self.format,
                wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm
            );
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for row in 0..h as usize {
                let start = row * padded as usize;
                for x in 0..w as usize {
                    let p = start + x * 4;
                    let (r, g, b) = if is_bgra {
                        (data[p + 2], data[p + 1], data[p])
                    } else {
                        (data[p], data[p + 1], data[p + 2])
                    };
                    rgb.push(r);
                    rgb.push(g);
                    rgb.push(b);
                }
            }
            drop(data);
            buffer.unmap();
            Captured { width: w, height: h, rgb }
        });

        frame.present();
        captured
    }

    /// Encode a copy of the rendered frame into a fresh staging buffer.
    /// Returns (buffer, padded_bytes_per_row, width, height).
    fn read_back(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> (wgpu::Buffer, u32, u32, u32) {
        let w = self.surface_config.width;
        let h = self.surface_config.height;
        let unpadded = w * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture staging"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        (buffer, padded, w, h)
    }
}

#[allow(clippy::too_many_arguments)]
fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    buffers: &[wgpu::VertexBufferLayout],
    format: wgpu::TextureFormat,
    fs_entry: &str,
    blend: wgpu::BlendState,
    depth_write: bool,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // No culling: we want both the outer and the inner surface.
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn new_instance_buffer(device: &wgpu::Device, capacity: usize, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
