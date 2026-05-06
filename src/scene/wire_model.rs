/// Tag for pre-baked snap candidates stored inside a WireModel.
/// Kept separate from `snap::SnapType` to avoid circular module dependencies.
#[derive(Clone, Copy, Debug)]
pub enum SnapHint {
    /// Geometric center of a circle, arc, or ellipse.
    Center,
    /// Point entity location.
    Node,
    /// 0 / 90 / 180 / 270 ° point on a circle/arc (within arc span).
    Quadrant,
    /// Insertion point of text or block.
    Insertion,
}

/// Geometric primitive used by the tangent-snap engine.
#[derive(Clone, Debug)]
pub enum TangentGeom {
    /// Infinite line through these two world-space points.
    Line { p1: [f32; 3], p2: [f32; 3] },
    /// Circle/arc.
    Circle { center: [f32; 3], radius: f32 },
}

/// A 1-D entity (line, arc, polyline) represented as an ordered set of
/// world-space points rendered as a quad strip (TriangleList).
///
/// Linetype is encoded as a GPU-side dash pattern so the CPU never needs to
/// split wires into per-dash segments.  `pattern_length = 0.0` means solid.
#[derive(Clone, Debug)]
pub struct WireModel {
    /// Unique identifier — the handle value as a decimal string.
    pub name: String,
    /// Ordered world-space positions forming a strip of quads.
    pub points: Vec<[f32; 3]>,
    /// RGBA colour in [0, 1].
    pub color: [f32; 4],
    /// Whether this wire is currently selected.
    #[allow(dead_code)]
    pub selected: bool,
    /// Total length of one pattern repeat (world units).  0 = solid line.
    pub pattern_length: f32,
    /// Up to 8 pattern elements: positive = dash length, negative = gap length.
    /// Unused slots must be 0.0 (acts as end-of-pattern sentinel in shader).
    pub pattern: [f32; 8],
    /// Rendered line width in screen pixels (half-width = line_weight_px / 2).
    pub line_weight_px: f32,
    /// ACI color index (1-255).  0 means true-color or unknown (no CTB lookup).
    pub aci: u8,
    /// Pre-baked snap candidates (Center, Node, Quadrant, Insertion).
    pub snap_pts: Vec<(glam::Vec3, SnapHint)>,
    /// Per-segment tangent geometry for Tangent snap.
    /// Line/Arc entities: 1 entry.  LwPolyline: 1 entry per segment.
    pub tangent_geoms: Vec<TangentGeom>,
    /// True polyline vertices used for Endpoint/Midpoint snap.
    /// Non-empty only for entities with distinct vertex positions (Line, LwPolyline).
    /// Empty for tessellated curves (Circle, Arc, Ellipse) which use snap_pts instead.
    pub key_vertices: Vec<[f32; 3]>,
    /// World-space 2-D bounding box [min_x, min_y, max_x, max_y].
    /// Set from acadrust `bounding_box()` in `tessellate_entity()`.
    /// Preview / interim wires use `UNBOUNDED_AABB` so they are never pre-rejected
    /// by the snap world-space filter.
    pub aabb: [f32; 4],
    /// When false the linetype pattern restarts at each NaN-separated segment
    /// (DXF PLINEGEN=0).  When true the pattern runs continuously (PLINEGEN=1).
    pub plinegen: bool,
    /// Paper-space bounding box [x0, y0, x1, y1] for GPU scissor clipping.
    /// Set only for viewport-projected wires in paper-space layouts.
    pub vp_scissor: Option<[f32; 4]>,
    /// Pre-triangulated solid fill: flat vertex list, 3 per triangle (world-offset applied).
    /// Non-empty only for PolyfaceMesh / PolygonMesh entities.
    pub fill_tris: Vec<[f32; 3]>,
}

impl WireModel {
    pub const WHITE: [f32; 4] = [1.00, 1.00, 1.00, 1.0];
    pub const CYAN: [f32; 4] = [0.25, 0.85, 1.00, 1.0];
    pub const SELECTED: [f32; 4] = [0.15, 0.55, 1.00, 1.0];
    /// Sentinel AABB that never rejects any snap query.
    pub const UNBOUNDED_AABB: [f32; 4] =
        [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::INFINITY];

    /// Create a solid wire (no dash pattern, 1px weight).
    pub fn solid(name: String, points: Vec<[f32; 3]>, color: [f32; 4], selected: bool) -> Self {
        Self {
            name,
            points,
            color,
            selected,
            aci: 0,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px: 1.0,
            snap_pts: vec![],
            tangent_geoms: vec![],
            key_vertices: vec![],
            aabb: Self::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: vec![],
        }
    }

    /// Return a clone with every point translated by `delta`.
    pub fn translated(&self, delta: glam::Vec3) -> Self {
        let mut out = self.clone();
        out.name = format!("preview_{}", self.name);
        out.color = Self::CYAN;
        out.selected = false;
        for p in &mut out.points {
            p[0] += delta.x;
            p[1] += delta.y;
            p[2] += delta.z;
        }
        out
    }

    /// Return a clone with every point rotated around `center` by `angle_rad`.
    pub fn rotated(&self, center: glam::Vec3, angle_rad: f32) -> Self {
        let (s, c) = angle_rad.sin_cos();
        let mut out = self.clone();
        out.name = format!("preview_{}", self.name);
        out.color = Self::CYAN;
        out.selected = false;
        for p in &mut out.points {
            let dx = p[0] - center.x;
            let dy = p[1] - center.y;
            p[0] = center.x + dx * c - dy * s;
            p[1] = center.y + dx * s + dy * c;
        }
        out
    }

    /// Return a clone with every point uniformly scaled from `center` by `factor`.
    pub fn scaled(&self, center: glam::Vec3, factor: f32) -> Self {
        let mut out = self.clone();
        out.name = format!("preview_{}", self.name);
        out.color = Self::CYAN;
        out.selected = false;
        for p in &mut out.points {
            p[0] = center.x + (p[0] - center.x) * factor;
            p[1] = center.y + (p[1] - center.y) * factor;
            p[2] = center.z + (p[2] - center.z) * factor;
        }
        out
    }

    /// Return a clone mirrored across the line through `p1`→`p2`.
    pub fn mirrored(&self, p1: glam::Vec3, p2: glam::Vec3) -> Self {
        let ax = p2.x - p1.x;
        let ay = p2.y - p1.y;
        let len2 = ax * ax + ay * ay;
        let mut out = self.clone();
        out.name = format!("preview_{}", self.name);
        out.color = Self::CYAN;
        out.selected = false;
        if len2 < 1e-12 {
            return out;
        }
        for p in &mut out.points {
            let dx = p[0] - p1.x;
            let dy = p[1] - p1.y;
            let t = (dx * ax + dy * ay) / len2;
            p[0] = p1.x + 2.0 * t * ax - dx;
            p[1] = p1.y + 2.0 * t * ay - dy;
        }
        out
    }

    /// Total arc-length of this wire (sum of segment lengths).
    #[allow(dead_code)]
    pub fn length(&self) -> f32 {
        self.points
            .windows(2)
            .map(|w| {
                let dx = w[1][0] - w[0][0];
                let dy = w[1][1] - w[0][1];
                let dz = w[1][2] - w[0][2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            })
            .sum()
    }
}

impl Default for WireModel {
    fn default() -> Self {
        Self {
            name: String::new(),
            points: Vec::new(),
            color: Self::WHITE,
            selected: false,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px: 1.0,
            aci: 0,
            snap_pts: Vec::new(),
            tangent_geoms: Vec::new(),
            key_vertices: Vec::new(),
            aabb: Self::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: Vec::new(),
        }
    }
}
