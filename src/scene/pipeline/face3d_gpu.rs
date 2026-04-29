// Face3D GPU buffer — batches all DXF 3DFACE entities into a single
// TriangleList buffer for efficient rendering.
//
// Each Face3D quad (4 corners) produces 2 triangles → 6 vertices.
// All entities are merged into one wgpu::Buffer → 1 draw call total.
//
// Vertex layout (28 bytes):
//   position  [f32; 3]   offset  0   12 B
//   color     [f32; 4]   offset 12   16 B
//                                ------
//                                 28 B / vertex

use crate::scene::wire_model::WireModel;
use iced::wgpu;
use iced::wgpu::util::DeviceExt;

// ── Vertex layout ──────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Face3DVertex {
    pub position: [f32; 3],
    pub color:    [f32; 4],
}

impl Face3DVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Face3DVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

// ── GPU handle ─────────────────────────────────────────────────────────────

pub struct Face3DGpu {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

impl Face3DGpu {
    /// Build a batched GPU buffer from Face3D wire models.
    ///
    /// Each WireModel's `key_vertices` holds the 4 corners in local space
    /// (world_offset already applied by tessellate.rs).  Two triangles are
    /// emitted per face: (p0,p1,p2) and (p0,p2,p3).
    pub fn from_wires(device: &wgpu::Device, wires: &[WireModel]) -> Self {
        let mut vertices: Vec<Face3DVertex> = Vec::with_capacity(wires.len() * 6);

        for wire in wires {
            // key_vertices has exactly 4 entries for Face3D (p0..p3).
            if wire.key_vertices.len() < 4 {
                continue;
            }
            let [r, g, b, a] = wire.color;
            let fill_color = [r * 0.45, g * 0.45, b * 0.45, a];
            let p = &wire.key_vertices;
            let v = |i: usize| Face3DVertex { position: p[i], color: fill_color };

            // Triangle 1: p0, p1, p2
            vertices.push(v(0));
            vertices.push(v(1));
            vertices.push(v(2));
            // Triangle 2: p0, p2, p3
            vertices.push(v(0));
            vertices.push(v(2));
            vertices.push(v(3));
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("face3d.vbuf"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertex_buffer,
            vertex_count: vertices.len() as u32,
        }
    }
}
