//! Sphere geometry.
//!
//! A plain UV (latitude/longitude) sphere. Each vertex stores only its unit
//! direction from the centre — that doubles as the rest position (scaled by
//! the radius in the shader) and as the radial normal. The eversion
//! deformation is applied entirely in the vertex shader, so the mesh itself
//! never changes after creation.

use bytemuck::{Pod, Zeroable};

/// A single vertex: its unit direction from the sphere centre.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub dir: [f32; 3],
}

impl Vertex {
    /// Vertex buffer layout: one vec3 at location 0.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        }
    }
}

/// CPU-side mesh data.
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// Build a UV sphere with the given number of latitude rings and
    /// longitude segments.
    pub fn uv_sphere(rings: u32, segments: u32) -> Mesh {
        use std::f32::consts::PI;

        let mut vertices = Vec::with_capacity(((rings + 1) * (segments + 1)) as usize);

        // Generate a grid of vertices. We duplicate the seam column
        // (segment == segments) so the longitude wrap is clean.
        for ring in 0..=rings {
            // latitude: 0 at the +Y pole, PI at the -Y pole.
            let v = ring as f32 / rings as f32;
            let lat = v * PI;
            let (sin_lat, cos_lat) = lat.sin_cos();

            for seg in 0..=segments {
                let u = seg as f32 / segments as f32;
                let lon = u * 2.0 * PI;
                let (sin_lon, cos_lon) = lon.sin_cos();

                // Unit direction on the sphere.
                let dir = [sin_lat * cos_lon, cos_lat, sin_lat * sin_lon];
                vertices.push(Vertex { dir });
            }
        }

        // Two triangles per grid quad.
        let mut indices = Vec::with_capacity((rings * segments * 6) as usize);
        let stride = segments + 1;
        for ring in 0..rings {
            for seg in 0..segments {
                let a = ring * stride + seg;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;

                // CCW winding when viewed from outside.
                indices.extend_from_slice(&[a, c, b]);
                indices.extend_from_slice(&[b, c, d]);
            }
        }

        Mesh { vertices, indices }
    }
}
