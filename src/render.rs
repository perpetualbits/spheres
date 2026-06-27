//! wgpu setup and the per-frame draw.
//!
//! Pipelines, in draw order:
//!   1. depth pre-pass — node spheres write depth only (so edges/rings are
//!      correctly occluded by spheres while the glass stays see-through).
//!   2. glass          — translucent node spheres (depth test off → see-through).
//!   3. ring           — data-driven Saturn-ring readouts (additive).
//!   4. edge           — camera-facing link ribbons (additive).
//! Then the HUD (labels + stats), depth-less, on top.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::Camera;
use crate::config;
use crate::hud::Hud;
use crate::sphere::{Mesh, Vertex};

// --- Instance types ------------------------------------------------------

/// A glass node sphere. `params` = (evert, focus, glow, spin).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    center_radius: [f32; 4],
    tint_alpha: [f32; 4],
    params: [f32; 4],
}

impl Instance {
    #[allow(clippy::too_many_arguments)]
    pub fn node(
        center: Vec3,
        radius: f32,
        tint: Vec3,
        alpha: f32,
        evert: f32,
        focus: f32,
        glow: f32,
        spin: f32,
    ) -> Self {
        Instance {
            center_radius: [center.x, center.y, center.z, radius],
            tint_alpha: [tint.x, tint.y, tint.z, alpha],
            params: [evert, focus, glow, spin],
        }
    }

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const A: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &A,
        }
    }
}

/// A Saturn-ring readout around a node.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RingInstance {
    center_inner: [f32; 4], // xyz center, w inner radius
    tint_outer: [f32; 4],   // rgb tint, w outer radius
    marker: [f32; 4],       // bits, marker_count, base_glow, marker_glow
    extra: [f32; 4],        // alpha, tilt, _, _
}

impl RingInstance {
    pub fn new(center: Vec3, node_radius: f32, tint: Vec3, bits: u32, alpha: f32) -> Self {
        RingInstance {
            center_inner: [
                center.x,
                center.y,
                center.z,
                node_radius * config::RING_INNER_FRAC,
            ],
            tint_outer: [tint.x, tint.y, tint.z, node_radius * config::RING_OUTER_FRAC],
            marker: [
                bits as f32,
                config::RING_MARKER_COUNT as f32,
                config::RING_BASE_GLOW,
                config::RING_MARKER_GLOW,
            ],
            extra: [alpha, config::RING_TILT_DEG.to_radians(), 0.0, 0.0],
        }
    }

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const A: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RingInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &A,
        }
    }
}

/// A glowing link ribbon between two nodes.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct EdgeInstance {
    a_width: [f32; 4],
    b_glow: [f32; 4],
    color_alpha: [f32; 4],
}

impl EdgeInstance {
    pub fn new(a: Vec3, b: Vec3, width: f32, glow: f32, color: Vec3, alpha: f32) -> Self {
        EdgeInstance {
            a_width: [a.x, a.y, a.z, width],
            b_glow: [b.x, b.y, b.z, glow],
            color_alpha: [color.x, color.y, color.z, alpha],
        }
    }

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const A: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<EdgeInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &A,
        }
    }
}

// --- Globals -------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_dir: [f32; 4],
    fold: [f32; 4],
    glass: [f32; 4],
    glass2: [f32; 4],
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
    depth_view: wgpu::TextureView,

    sphere_vb: wgpu::Buffer,
    sphere_ib: wgpu::Buffer,
    sphere_n: u32,
    ring_vb: wgpu::Buffer,
    ring_ib: wgpu::Buffer,
    ring_n: u32,
    edge_vb: wgpu::Buffer,
    edge_ib: wgpu::Buffer,
    edge_n: u32,

    node_inst: wgpu::Buffer,
    node_cap: usize,
    ring_inst: wgpu::Buffer,
    ring_cap: usize,
    edge_inst: wgpu::Buffer,
    edge_cap: usize,

    depth_pipeline: wgpu::RenderPipeline,
    glass_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,

    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).expect("create surface");

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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // --- Meshes ---
        let sphere = Mesh::uv_sphere(config::SPHERE_RINGS, config::SPHERE_SEGMENTS);
        let (sphere_vb, sphere_ib, sphere_n) =
            upload_mesh(&device, bytemuck::cast_slice(&sphere.vertices), &sphere.indices, "sphere");

        let (ring_v, ring_i) = ring_mesh(96);
        let (ring_vb, ring_ib, ring_n) =
            upload_mesh(&device, bytemuck::cast_slice(&ring_v), &ring_i, "ring");

        let (edge_v, edge_i) = edge_mesh();
        let (edge_vb, edge_ib, edge_n) =
            upload_mesh(&device, bytemuck::cast_slice(&edge_v), &edge_i, "edge");

        // --- Instance buffers ---
        let node_cap = 256;
        let ring_cap = 256;
        let edge_cap = 256;
        let node_inst = inst_buffer::<Instance>(&device, node_cap, "node inst");
        let ring_inst = inst_buffer::<RingInstance>(&device, ring_cap, "ring inst");
        let edge_inst = inst_buffer::<EdgeInstance>(&device, edge_cap, "edge inst");

        // --- Uniforms ---
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let sphere_vl = Vertex::layout();
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth prepass"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[sphere_vl.clone(), Instance::layout()],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: tri_prim(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let glass_pipeline = make_pipeline(MakePipeline {
            device: &device,
            layout: &layout,
            shader: &shader,
            buffers: &[sphere_vl.clone(), Instance::layout()],
            vs: "vs_main",
            fs: "fs_glass",
            format,
            blend: wgpu::BlendState::ALPHA_BLENDING,
            // Depth test OFF so glass stays see-through; it never writes depth.
            depth_compare: wgpu::CompareFunction::Always,
            label: "glass",
        });
        let ring_pipeline = make_pipeline(MakePipeline {
            device: &device,
            layout: &layout,
            shader: &shader,
            buffers: &[ring_vertex_layout(), RingInstance::layout()],
            vs: "vs_ring",
            fs: "fs_ring",
            format,
            blend: additive(),
            depth_compare: wgpu::CompareFunction::LessEqual,
            label: "ring",
        });
        let edge_pipeline = make_pipeline(MakePipeline {
            device: &device,
            layout: &layout,
            shader: &shader,
            buffers: &[edge_vertex_layout(), EdgeInstance::layout()],
            vs: "vs_edge",
            fs: "fs_edge",
            format,
            blend: additive(),
            depth_compare: wgpu::CompareFunction::LessEqual,
            label: "edge",
        });

        let depth_view = create_depth_view(&device, width, height);

        Renderer {
            surface,
            device,
            queue,
            surface_config,
            format,
            depth_view,
            sphere_vb,
            sphere_ib,
            sphere_n,
            ring_vb,
            ring_ib,
            ring_n,
            edge_vb,
            edge_ib,
            edge_n,
            node_inst,
            node_cap,
            ring_inst,
            ring_cap,
            edge_inst,
            edge_cap,
            depth_pipeline,
            glass_pipeline,
            ring_pipeline,
            edge_pipeline,
            uniform_buffer,
            bind_group,
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

    fn build_globals(&self, camera: &Camera, eye_z: f32, time: f32) -> Globals {
        let view_proj = camera.view_proj(eye_z);
        let eye = camera.eye(eye_z);
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
            glass2: [config::GLASS_FOCUS_CLARITY, config::GLASS_INNER_BOOST, 0.0, 0.0],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        camera: &Camera,
        eye_z: f32,
        time: f32,
        clear: [f64; 4],
        nodes: &[Instance],
        rings: &[RingInstance],
        edges: &[EdgeInstance],
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
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let globals = self.build_globals(camera, eye_z, time);
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&globals));

        // Upload instances (growing buffers as needed).
        upload_instances(&self.device, &self.queue, &mut self.node_inst, &mut self.node_cap, nodes, "node inst");
        upload_instances(&self.device, &self.queue, &mut self.ring_inst, &mut self.ring_cap, rings, "ring inst");
        upload_instances(&self.device, &self.queue, &mut self.edge_inst, &mut self.edge_cap, edges, "edge inst");

        hud.prepare(&self.device, &self.queue).expect("hud prepare");

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        // Pass A: depth pre-pass (spheres → depth only).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth prepass"),
                color_attachments: &[],
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
            if !nodes.is_empty() {
                pass.set_pipeline(&self.depth_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.sphere_vb.slice(..));
                pass.set_vertex_buffer(1, self.node_inst.slice(..));
                pass.set_index_buffer(self.sphere_ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.sphere_n, 0, 0..nodes.len() as u32);
            }
        }

        // Pass B: colour — glass, then rings, then edges.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("color pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
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
            pass.set_bind_group(0, &self.bind_group, &[]);

            if !nodes.is_empty() {
                pass.set_pipeline(&self.glass_pipeline);
                pass.set_vertex_buffer(0, self.sphere_vb.slice(..));
                pass.set_vertex_buffer(1, self.node_inst.slice(..));
                pass.set_index_buffer(self.sphere_ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.sphere_n, 0, 0..nodes.len() as u32);
            }
            if !edges.is_empty() {
                pass.set_pipeline(&self.edge_pipeline);
                pass.set_vertex_buffer(0, self.edge_vb.slice(..));
                pass.set_vertex_buffer(1, self.edge_inst.slice(..));
                pass.set_index_buffer(self.edge_ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.edge_n, 0, 0..edges.len() as u32);
            }
            if !rings.is_empty() {
                pass.set_pipeline(&self.ring_pipeline);
                pass.set_vertex_buffer(0, self.ring_vb.slice(..));
                pass.set_vertex_buffer(1, self.ring_inst.slice(..));
                pass.set_index_buffer(self.ring_ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.ring_n, 0, 0..rings.len() as u32);
            }
        }

        // Pass C: HUD (labels + stats), no depth.
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
                    let (r, gc, b) = if is_bgra {
                        (data[p + 2], data[p + 1], data[p])
                    } else {
                        (data[p], data[p + 1], data[p + 2])
                    };
                    rgb.push(r);
                    rgb.push(gc);
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

    fn read_back(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> (wgpu::Buffer, u32, u32, u32) {
        let w = self.surface_config.width;
        let h = self.surface_config.height;
        let padded = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
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

// --- pipeline / buffer helpers ------------------------------------------

struct MakePipeline<'a> {
    device: &'a wgpu::Device,
    layout: &'a wgpu::PipelineLayout,
    shader: &'a wgpu::ShaderModule,
    buffers: &'a [wgpu::VertexBufferLayout<'a>],
    vs: &'a str,
    fs: &'a str,
    format: wgpu::TextureFormat,
    blend: wgpu::BlendState,
    depth_compare: wgpu::CompareFunction,
    label: &'a str,
}

fn make_pipeline(m: MakePipeline) -> wgpu::RenderPipeline {
    m.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(m.label),
        layout: Some(m.layout),
        vertex: wgpu::VertexState {
            module: m.shader,
            entry_point: Some(m.vs),
            buffers: m.buffers,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: m.shader,
            entry_point: Some(m.fs),
            targets: &[Some(wgpu::ColorTargetState {
                format: m.format,
                blend: Some(m.blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: tri_prim(),
        depth_stencil: Some(depth_state(false, m.depth_compare)),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn tri_prim() -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,
        polygon_mode: wgpu::PolygonMode::Fill,
        unclipped_depth: false,
        conservative: false,
    }
}

fn depth_state(write: bool, compare: wgpu::CompareFunction) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

fn additive() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

// Each entry: (cos, sin, side) where side 0 = inner ring, 1 = outer ring.
fn ring_mesh(segments: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut verts = Vec::new();
    for i in 0..=segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        verts.push([c, s, 0.0]); // inner
        verts.push([c, s, 1.0]); // outer
    }
    let mut idx = Vec::new();
    for i in 0..segments {
        let b = i * 2;
        idx.extend_from_slice(&[b, b + 1, b + 2, b + 2, b + 1, b + 3]);
    }
    (verts, idx)
}

fn ring_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const A: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    wgpu::VertexBufferLayout {
        array_stride: 12,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &A,
    }
}

// (along 0..1, across -1..1).
fn edge_mesh() -> (Vec<[f32; 2]>, Vec<u32>) {
    (
        vec![[0.0, -1.0], [1.0, -1.0], [1.0, 1.0], [0.0, 1.0]],
        vec![0, 1, 2, 0, 2, 3],
    )
}

fn edge_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const A: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
    wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &A,
    }
}

fn upload_mesh(
    device: &wgpu::Device,
    vertex_bytes: &[u8],
    indices: &[u32],
    label: &str,
) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: vertex_bytes,
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vb, ib, indices.len() as u32)
}

fn inst_buffer<T>(device: &wgpu::Device, capacity: usize, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity * std::mem::size_of::<T>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn upload_instances<T: Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    cap: &mut usize,
    data: &[T],
    label: &str,
) {
    if data.len() > *cap {
        *cap = data.len().next_power_of_two();
        *buffer = inst_buffer::<T>(device, *cap, label);
    }
    if !data.is_empty() {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(data));
    }
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
