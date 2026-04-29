// Wire GPU buffers — quad (TriangleList) rendering for thick lines.
//
// Each segment [A→B] emits 6 vertices (2 triangles).  Every vertex carries
// both endpoints so the vertex shader can compute the screen-space
// perpendicular direction and expand the quad to the correct pixel width.
//
// NaN sentinel: text glyphs pack multiple disconnected strokes into one
// WireModel, separated by [NaN, NaN, NaN] points. Segments where either
// endpoint contains NaN are silently skipped during upload.
//
// Vertex layout (96 bytes, stride = 96):
//   pos_a          [f32; 3]   offset  0   12 B  — segment start (world)
//   pos_b          [f32; 3]   offset 12   12 B  — segment end   (world)
//   which_end      f32        offset 24    4 B  — 0.0 = A end, 1.0 = B end
//   side           f32        offset 28    4 B  — ±1.0 perpendicular side
//   color          [f32; 4]   offset 32   16 B  — RGBA [0,1]
//   distance       f32        offset 48    4 B  — arc-length from wire start
//   half_width     f32        offset 52    4 B  — half line width in pixels
//   pattern_length f32        offset 56    4 B  — dash pattern total length
//   _pad           f32        offset 60    4 B
//   pat0           [f32; 4]   offset 64   16 B  — pattern elements 0-3
//   pat1           [f32; 4]   offset 80   16 B  — pattern elements 4-7
//                                         ------
//                                          96 B / vertex

use crate::scene::wire_model::WireModel;
use iced::wgpu;
use iced::wgpu::util::DeviceExt;

// ── Vertex layout ─────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WireVertex {
    pub pos_a: [f32; 3],
    pub pos_b: [f32; 3],
    pub which_end: f32,
    pub side: f32,
    pub color: [f32; 4],
    pub distance: f32,
    pub half_width: f32,
    pub pattern_length: f32,
    pub _pad: f32,
    pub pat0: [f32; 4],
    pub pat1: [f32; 4],
}

impl WireVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WireVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                }, // pos_a
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                }, // pos_b
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                }, // which_end
                wgpu::VertexAttribute {
                    offset: 28,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                }, // side
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                }, // color
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                }, // distance
                wgpu::VertexAttribute {
                    offset: 52,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32,
                }, // half_width
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32,
                }, // pattern_length
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                }, // pat0
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                }, // pat1
            ],
        }
    }
}

// ── GPU handle ────────────────────────────────────────────────────────────

pub struct WireGpu {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

impl WireGpu {
    pub fn new(device: &wgpu::Device, wire: &WireModel) -> Self {
        Self::build(device, wire, wire.color)
    }

    /// Creates a ghost copy with `alpha` applied on top of the wire's own alpha.
    #[allow(dead_code)]
    pub fn new_ghost(device: &wgpu::Device, wire: &WireModel, alpha: f32) -> Self {
        let [r, g, b, a] = wire.color;
        Self::build(device, wire, [r, g, b, a * alpha])
    }

    /// Merge multiple WireModels into GPU buffers chunked to fit the 256 MB GPU limit.
    /// Each wire keeps its own color and pattern — they're stored per-vertex.
    /// Returns an empty Vec if the combined vertex list is empty.
    pub fn from_batch(device: &wgpu::Device, wires: &[WireModel]) -> Vec<Self> {
        let total_segs: usize = wires.iter().map(|w| w.points.len().saturating_sub(1)).sum();
        if total_segs == 0 {
            return vec![];
        }
        let mut vertices: Vec<WireVertex> = Vec::with_capacity(total_segs * 6);

        for wire in wires {
            let color = wire.color;
            let pat0 = [wire.pattern[0], wire.pattern[1], wire.pattern[2], wire.pattern[3]];
            let pat1 = [wire.pattern[4], wire.pattern[5], wire.pattern[6], wire.pattern[7]];
            let half_width = wire.line_weight_px * 0.5;
            let n = wire.points.len();

            // Cumulative arc-length per point.
            let mut dists = vec![0.0_f32; n];
            for i in 1..n {
                let p = wire.points[i - 1];
                let q = wire.points[i];
                if !p[0].is_finite() || !q[0].is_finite() {
                    dists[i] = dists[i - 1];
                } else {
                    let dx = q[0] - p[0];
                    let dy = q[1] - p[1];
                    let dz = q[2] - p[2];
                    dists[i] = dists[i - 1] + (dx * dx + dy * dy + dz * dz).sqrt();
                }
            }

            for i in 0..n.saturating_sub(1) {
                let a = wire.points[i];
                let b = wire.points[i + 1];
                if !a[0].is_finite() || !a[1].is_finite() || !a[2].is_finite()
                    || !b[0].is_finite() || !b[1].is_finite() || !b[2].is_finite()
                {
                    continue;
                }
                let dist_a = dists[i];
                let dist_b = dists[i + 1];
                let make = |which_end: f32, side: f32| -> WireVertex {
                    let dist = if which_end < 0.5 { dist_a } else { dist_b };
                    WireVertex {
                        pos_a: a, pos_b: b, which_end, side, color,
                        distance: dist, half_width,
                        pattern_length: wire.pattern_length,
                        _pad: 0.0, pat0, pat1,
                    }
                };
                vertices.push(make(0.0, -1.0));
                vertices.push(make(1.0, -1.0));
                vertices.push(make(1.0,  1.0));
                vertices.push(make(0.0, -1.0));
                vertices.push(make(1.0,  1.0));
                vertices.push(make(0.0,  1.0));
            }
        }

        if vertices.is_empty() {
            return vec![];
        }

        // GPU max buffer size is 256 MB; chunk to stay within the limit.
        const MAX_VERTS: usize = 268_435_456 / std::mem::size_of::<WireVertex>();

        vertices
            .chunks(MAX_VERTS)
            .enumerate()
            .map(|(i, chunk)| {
                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("wire.batch.vbuf.{i}")),
                    contents: bytemuck::cast_slice(chunk),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                Self {
                    vertex_buffer,
                    vertex_count: chunk.len() as u32,
                }
            })
            .collect()
    }

    fn build(device: &wgpu::Device, wire: &WireModel, color: [f32; 4]) -> Self {
        let pat0 = [
            wire.pattern[0],
            wire.pattern[1],
            wire.pattern[2],
            wire.pattern[3],
        ];
        let pat1 = [
            wire.pattern[4],
            wire.pattern[5],
            wire.pattern[6],
            wire.pattern[7],
        ];
        let half_width = wire.line_weight_px * 0.5;

        let n = wire.points.len();
        let seg_count = if n >= 2 { n - 1 } else { 0 };
        let mut vertices: Vec<WireVertex> = Vec::with_capacity(seg_count * 6);

        // Precompute cumulative arc-length at each point.
        // NaN points do not contribute to arc length — they are separator sentinels.
        let mut dists = vec![0.0_f32; n];
        for i in 1..n {
            let p = wire.points[i - 1];
            let q = wire.points[i];
            // If either point is non-finite (NaN sentinel or ±inf from overflow),
            // keep the same distance — the segment will be skipped anyway.
            if !p[0].is_finite() || !q[0].is_finite() {
                dists[i] = dists[i - 1];
            } else {
                let dx = q[0] - p[0];
                let dy = q[1] - p[1];
                let dz = q[2] - p[2];
                dists[i] = dists[i - 1] + (dx * dx + dy * dy + dz * dz).sqrt();
            }
        }

        for i in 0..seg_count {
            let a = wire.points[i];
            let b = wire.points[i + 1];

            // Skip segments where either endpoint is non-finite (NaN sentinels
            // from disconnected glyph strokes, or ±inf from Ray/XLine far-point
            // overflow when the direction vector is very large).
            if !a[0].is_finite()
                || !a[1].is_finite()
                || !a[2].is_finite()
                || !b[0].is_finite()
                || !b[1].is_finite()
                || !b[2].is_finite()
            {
                continue;
            }

            let dist_a = dists[i];
            let dist_b = dists[i + 1];

            let make = |which_end: f32, side: f32| -> WireVertex {
                let dist = if which_end < 0.5 { dist_a } else { dist_b };
                WireVertex {
                    pos_a: a,
                    pos_b: b,
                    which_end,
                    side,
                    color,
                    distance: dist,
                    half_width,
                    pattern_length: wire.pattern_length,
                    _pad: 0.0,
                    pat0,
                    pat1,
                }
            };

            // Triangle 1: A(-1), B(-1), B(+1)
            vertices.push(make(0.0, -1.0));
            vertices.push(make(1.0, -1.0));
            vertices.push(make(1.0, 1.0));
            // Triangle 2: A(-1), B(+1), A(+1)
            vertices.push(make(0.0, -1.0));
            vertices.push(make(1.0, 1.0));
            vertices.push(make(0.0, 1.0));
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("wire.vbuf.{}", wire.name)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertex_buffer,
            vertex_count: vertices.len() as u32,
        }
    }
}
