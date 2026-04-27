// Unified tessellation for all truck topology levels.
//
//   Vertex              → TruckTessResult::Point
//   Edge                → TruckTessResult::Lines   (curve sampled with tolerance)
//   Wire                → TruckTessResult::Lines   (edges joined, no duplicate junctions)
//   Face / Shell / Solid → TruckTessResult::Mesh    (triangle mesh via truck-meshalgo)
//
// The caller (acad_to_truck / tessellate) decides which function to call based on
// the truck topology level of the object.

use truck_meshalgo::tessellation::{MeshableShape, MeshedShape};
use truck_modeling::base::{BoundedCurve, ParameterDivision1D};
use truck_modeling::{Edge, Shell, Solid, Vertex, Wire};
use truck_polymesh::PolygonMesh;

use crate::scene::mesh_model::MeshModel;

// Chord-height tolerance used for adaptive curve sampling (world units).
const CURVE_TOL: f64 = 0.005;
// Triangle mesh tolerance (world units).
pub const MESH_TOL: f64 = 0.01;

// ── Public result type ────────────────────────────────────────────────────

/// Output of any truck topology tessellation.
#[allow(dead_code)]
pub enum TruckTessResult {
    /// A single world-space point (from Vertex).
    Point([f32; 3]),
    /// An ordered sequence of points forming a polyline (from Edge or Wire).
    Lines(Vec<[f32; 3]>),
    /// A triangle mesh (from Face, Shell, or Solid).
    Mesh {
        verts: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
    },
}

// ── Shared offset helper ──────────────────────────────────────────────────

/// Convert a truck f64 point to local f32 by subtracting world_offset in f64
/// before truncating.  This preserves sub-unit decimal precision even when the
/// raw world coordinates are in the millions (e.g. Turkish UTM).
#[inline]
fn to_local(x: f64, y: f64, z: f64, off: [f64; 3]) -> [f32; 3] {
    [(x - off[0]) as f32, (y - off[1]) as f32, (z - off[2]) as f32]
}

// ── Vertex ────────────────────────────────────────────────────────────────

pub fn tessellate_vertex(v: &Vertex, offset: [f64; 3]) -> TruckTessResult {
    let p = v.point();
    TruckTessResult::Point(to_local(p.x, p.y, p.z, offset))
}

// ── Edge ──────────────────────────────────────────────────────────────────

pub fn tessellate_edge(e: &Edge, offset: [f64; 3]) -> TruckTessResult {
    // oriented_curve() respects the edge direction (inverts if needed).
    let curve = e.oriented_curve();
    let (t0, t1) = curve.range_tuple();
    // parameter_division samples adaptively: fewer points on straight segments,
    // more on tight curves, all within CURVE_TOL chord-height error.
    let (_, pts) = curve.parameter_division((t0, t1), CURVE_TOL);
    let lines = pts
        .iter()
        .map(|p| to_local(p.x, p.y, p.z, offset))
        .collect();
    TruckTessResult::Lines(lines)
}

// ── Wire ──────────────────────────────────────────────────────────────────

pub fn tessellate_wire(w: &Wire, offset: [f64; 3]) -> TruckTessResult {
    let mut pts: Vec<[f32; 3]> = Vec::new();
    for edge in w.edge_iter() {
        if let TruckTessResult::Lines(ep) = tessellate_edge(edge, offset) {
            if pts.is_empty() {
                pts = ep;
            } else {
                // Skip the first point of each continuation edge to avoid
                // duplicating the shared junction vertex.
                pts.extend_from_slice(&ep[1..]);
            }
        }
    }
    TruckTessResult::Lines(pts)
}

// ── Shell ─────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn tessellate_shell(s: &Shell, offset: [f64; 3]) -> TruckTessResult {
    let meshed = s.triangulation(MESH_TOL);
    polygon_to_result(meshed.to_polygon(), offset)
}

// ── Solid ─────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn tessellate_solid(s: &Solid, offset: [f64; 3]) -> TruckTessResult {
    let meshed = s.triangulation(MESH_TOL);
    polygon_to_result(meshed.to_polygon(), offset)
}

// ── MeshModel helper ──────────────────────────────────────────────────────

/// Convert a TruckTessResult::Mesh into a MeshModel (fills name/color later).
pub fn tess_to_mesh_model(
    result: TruckTessResult,
    name: String,
    color: [f32; 4],
    selected: bool,
) -> Option<MeshModel> {
    match result {
        TruckTessResult::Mesh {
            verts,
            normals,
            indices,
        } => Some(MeshModel {
            name,
            verts,
            normals,
            indices,
            color,
            selected,
        }),
        _ => None,
    }
}

// ── Internal ─────────────────────────────────────────────────────────────

fn polygon_to_result(mesh: PolygonMesh, offset: [f64; 3]) -> TruckTessResult {
    let verts: Vec<[f32; 3]> = mesh
        .positions()
        .iter()
        .map(|p| to_local(p.x, p.y, p.z, offset))
        .collect();

    // Per-vertex normals: if the mesh has normals, map each triangle vertex's
    // normal index back to the normals array.  Fall back to empty if absent.
    let mesh_normals = mesh.normals();

    let indices: Vec<u32> = mesh
        .tri_faces()
        .iter()
        .flat_map(|tri| [tri[0].pos as u32, tri[1].pos as u32, tri[2].pos as u32])
        .collect();

    // Build a per-position normal by averaging normals of all faces that share it.
    // If mesh has no normals, leave the Vec empty.
    let normals: Vec<[f32; 3]> = if !mesh_normals.is_empty() {
        let n = verts.len();
        let mut acc = vec![[0.0_f32; 3]; n];
        let mut cnt = vec![0u32; n];
        for tri in mesh.tri_faces() {
            for sv in tri {
                let pos_idx = sv.pos;
                if let Some(nor_idx) = sv.nor {
                    if let Some(nv) = mesh_normals.get(nor_idx) {
                        acc[pos_idx][0] += nv.x as f32;
                        acc[pos_idx][1] += nv.y as f32;
                        acc[pos_idx][2] += nv.z as f32;
                        cnt[pos_idx] += 1;
                    }
                }
            }
        }
        acc.iter()
            .zip(cnt.iter())
            .map(|(s, &c)| {
                if c == 0 {
                    [0.0, 1.0, 0.0]
                } else {
                    let inv = 1.0 / c as f32;
                    let nx = s[0] * inv;
                    let ny = s[1] * inv;
                    let nz = s[2] * inv;
                    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-9);
                    [nx / len, ny / len, nz / len]
                }
            })
            .collect()
    } else {
        vec![]
    };

    TruckTessResult::Mesh {
        verts,
        normals,
        indices,
    }
}
