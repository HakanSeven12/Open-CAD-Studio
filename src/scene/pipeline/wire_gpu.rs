// Wire GPU buffers — instanced quad rendering for thick lines.
//
// Each segment [A→B] is one INSTANCE; the vertex shader expands a 6-vertex
// unit quad whose corners are derived from `@builtin(vertex_index)`. This
// cuts upload bandwidth by ~6.5× versus the old layout (which duplicated
// the segment payload across six vertex records).
//
// NaN sentinel: text glyphs pack multiple disconnected strokes into one
// WireModel, separated by [NaN, NaN, NaN] points. Segments where either
// endpoint contains NaN are silently skipped during emission.
//
// Instance layout (88 bytes, stride = 88, step_mode = Instance):
//   pos_a          [f32; 3]   offset  0   12 B  — segment start (world)
//   pos_b          [f32; 3]   offset 12   12 B  — segment end   (world)
//   color          [f32; 4]   offset 24   16 B  — RGBA [0,1]
//   distance_a     f32        offset 40    4 B  — arc-length at endpoint A
//   distance_b     f32        offset 44    4 B  — arc-length at endpoint B
//   half_width     f32        offset 48    4 B  — half line width in pixels
//   pattern_length f32        offset 52    4 B  — dash pattern total length
//   pat0           [f32; 4]   offset 56   16 B  — pattern elements 0-3
//   pat1           [f32; 4]   offset 72   16 B  — pattern elements 4-7
//                                          ------
//                                           88 B / instance

use crate::scene::wire_model::WireModel;
use iced::wgpu;
use rayon::prelude::*;

/// Allocate a VERTEX buffer with `mapped_at_creation` and write `data` directly
/// into the mapped slice. Skips the intermediate staging copy that
/// `create_buffer_init` performs and avoids holding a second `Vec` worth of
/// memory during upload — meaningful on cold open where wire buffers can run
/// into the hundreds of MB.
fn instance_buffer_mapped(
    device: &wgpu::Device,
    label: &str,
    data: &[WireInstance],
) -> wgpu::Buffer {
    let bytes: &[u8] = bytemuck::cast_slice(data);
    // wgpu rejects size-0 buffers; the renderer already guards `instance_count`
    // before issuing a draw, so a placeholder allocation is fine here.
    let size = bytes.len().max(std::mem::size_of::<WireInstance>()) as u64;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    {
        let mut view = buf.slice(..).get_mapped_range_mut();
        view[..bytes.len()].copy_from_slice(bytes);
    }
    buf.unmap();
    buf
}

// ── Instance layout ───────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WireInstance {
    pub pos_a: [f32; 3],
    pub pos_b: [f32; 3],
    pub color: [f32; 4],
    pub distance_a: f32,
    pub distance_b: f32,
    pub half_width: f32,
    pub pattern_length: f32,
    pub pat0: [f32; 4],
    pub pat1: [f32; 4],
}

impl WireInstance {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WireInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
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
                    format: wgpu::VertexFormat::Float32x4,
                }, // color
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                }, // distance_a
                wgpu::VertexAttribute {
                    offset: 44,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                }, // distance_b
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                }, // half_width
                wgpu::VertexAttribute {
                    offset: 52,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32,
                }, // pattern_length
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                }, // pat0
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                }, // pat1
            ],
        }
    }
}

// ── GPU handle ────────────────────────────────────────────────────────────

pub struct WireGpu {
    pub instance_buffer: wgpu::Buffer,
    pub instance_count: u32,
    /// Paper-space bbox [x0, y0, x1, y1] for GPU scissor clipping.
    /// Set only for viewport-projected wires; None for regular wires.
    pub vp_scissor: Option<[f32; 4]>,
}

/// Expand one `WireModel` into its per-segment instance stream (1 instance per
/// finite segment). Pulled out so both the single-wire and batched paths share
/// the same emission logic, and so the batched path can `par_iter` across
/// wires on cold open.
fn emit_wire_instances(wire: &WireModel, color: [f32; 4]) -> Vec<WireInstance> {
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
    let seg_count = n.saturating_sub(1);
    if seg_count == 0 {
        return Vec::new();
    }

    let mut dists = vec![0.0_f32; n];
    for i in 1..n {
        let p = wire.points[i - 1];
        let q = wire.points[i];
        if !p[0].is_finite() || !q[0].is_finite() {
            // plinegen=false: reset to 0 at the first real point after a NaN separator.
            dists[i] = if !wire.plinegen && !p[0].is_finite() && q[0].is_finite() {
                0.0
            } else {
                dists[i - 1]
            };
        } else {
            let dx = q[0] - p[0];
            let dy = q[1] - p[1];
            let dz = q[2] - p[2];
            dists[i] = dists[i - 1] + (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }

    let mut instances: Vec<WireInstance> = Vec::with_capacity(seg_count);
    for i in 0..seg_count {
        let a = wire.points[i];
        let b = wire.points[i + 1];
        if !a[0].is_finite()
            || !a[1].is_finite()
            || !a[2].is_finite()
            || !b[0].is_finite()
            || !b[1].is_finite()
            || !b[2].is_finite()
        {
            continue;
        }
        instances.push(WireInstance {
            pos_a: a,
            pos_b: b,
            color,
            distance_a: dists[i],
            distance_b: dists[i + 1],
            half_width,
            pattern_length: wire.pattern_length,
            pat0,
            pat1,
        });
    }
    instances
}

impl WireGpu {
    pub fn new(device: &wgpu::Device, wire: &WireModel) -> Self {
        let mut g = Self::build(device, wire, wire.color);
        g.vp_scissor = wire.vp_scissor;
        g
    }

    /// Merge multiple WireModels into GPU instance buffers, chunked to fit the
    /// 256 MB GPU limit. Each wire keeps its own color and pattern — they live
    /// per-instance.
    pub fn from_batch(device: &wgpu::Device, wires: &[WireModel]) -> Vec<Self> {
        let total_segs: usize = wires.iter().map(|w| w.points.len().saturating_sub(1)).sum();
        if total_segs == 0 {
            return vec![];
        }

        // Parallel per-wire instance emission. Each wire's stream is
        // independent — `block_cache` groups wires by style upstream, so order
        // within a batch does not affect correctness. `reduce` concatenates the
        // per-wire Vec<WireInstance> chunks while letting rayon spread work
        // across cores.
        let instances: Vec<WireInstance> = wires
            .par_iter()
            .map(|wire| emit_wire_instances(wire, wire.color))
            .reduce(Vec::new, |mut acc, mut chunk| {
                if acc.is_empty() {
                    chunk
                } else if chunk.is_empty() {
                    acc
                } else {
                    acc.append(&mut chunk);
                    acc
                }
            });

        if instances.is_empty() {
            return vec![];
        }

        // GPU max buffer size is 256 MB; chunk to stay within the limit.
        const MAX_INSTANCES: usize = 268_435_456 / std::mem::size_of::<WireInstance>();

        instances
            .chunks(MAX_INSTANCES)
            .enumerate()
            .map(|(i, chunk)| {
                let label = format!("wire.batch.ibuf.{i}");
                let instance_buffer = instance_buffer_mapped(device, &label, chunk);
                Self {
                    instance_buffer,
                    instance_count: chunk.len() as u32,
                    vp_scissor: None,
                }
            })
            .collect()
    }

    fn build(device: &wgpu::Device, wire: &WireModel, color: [f32; 4]) -> Self {
        let instances = emit_wire_instances(wire, color);
        let label = format!("wire.ibuf.{}", wire.name);
        let instance_buffer = instance_buffer_mapped(device, &label, &instances);

        Self {
            instance_buffer,
            instance_count: instances.len() as u32,
            vp_scissor: None,
        }
    }
}
