// acadrust -> truck topology conversion layer.

use acadrust::{CadDocument, EntityType};
use glam::Vec3;
use truck_modeling::{Edge, Solid, Vertex, Wire};

use crate::entities::traits::EntityTypeOps;
use crate::scene::wire_model::{SnapHint, TangentGeom};

/// One group of glyph strokes with its world-space origin stored in f64.
/// Strokes are in glyph-local space (origin = [0,0]) so that the large
/// world offset can be subtracted with f64 precision in tessellate.rs.
pub struct TextStroke {
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub origin:  [f64; 2],
}

#[allow(dead_code)]
pub enum TruckObject {
    Point(Vertex),
    Curve(Edge),
    Contour(Wire),
    Text(Vec<TextStroke>),
    /// Pre-computed NaN-separated 3-D point list (leader lines, arrowheads, etc.).
    Lines(Vec<[f32; 3]>),
    /// Like Lines but linetype pattern restarts at each NaN-separated segment (plinegen=false).
    SegmentedLines(Vec<[f32; 3]>),
    Volume(Solid),
}

pub struct TruckEntity {
    pub object: TruckObject,
    pub snap_pts: Vec<(Vec3, SnapHint)>,
    pub tangent_geoms: Vec<TangentGeom>,
    pub key_vertices: Vec<[f32; 3]>,
    /// Pre-triangulated fill geometry: flat list of [f32;3] vertices, 3 per triangle.
    /// Non-empty for mesh-like entities (PolyfaceMesh, PolygonMesh) that need solid fill.
    pub fill_tris: Vec<[f32; 3]>,
}

pub fn convert(entity: &EntityType, document: &CadDocument) -> Option<TruckEntity> {
    entity.to_truck_entity(document)
}
