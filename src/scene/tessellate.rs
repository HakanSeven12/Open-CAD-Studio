// Tessellation — convert acadrust EntityType to GPU-ready WireModel or MeshModel.
//
// Flow:
//   EntityType
//     ↓  acad_to_truck::convert()
//   TruckEntity  { object: TruckObject, snap_pts, tangent_geoms, key_vertices }
//     ↓  truck_tess::tessellate_*()
//   TruckTessResult::Lines → WireModel
//   TruckTessResult::Point → WireModel (small cross)
//   TruckTessResult::Mesh  → MeshModel
//   TruckObject::Text      → one WireModel per glyph stroke (elevation from entity Z)
//
// Entities not handled by acad_to_truck (Viewport, Hatch, …) are tessellated
// by the legacy geometry() path so nothing regresses.

use acadrust::entities::{Dimension, Leader, MultiLeader, MultiLeaderPathType, Text};
use acadrust::types::{Color as AcadColor, Vector3};
use acadrust::{CadDocument, EntityType, Handle};
use glam::Vec3;

use crate::scene::acad_to_truck::{convert, TruckObject};
use crate::scene::mesh_model::MeshModel;
use crate::scene::truck_tess::{
    self, tessellate_edge, tessellate_solid, tessellate_vertex, tessellate_wire, TruckTessResult,
};
use crate::scene::wire_model::{SnapHint, TangentGeom, WireModel};

// ── Colour helper ──────────────────────────────────────────────────────────

/// Convert an acadrust Color (ACI index or true-color) to a GPU RGBA value.
pub fn aci_to_rgba(color: &AcadColor) -> [f32; 4] {
    if let Some((r, g, b)) = color.rgb() {
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    } else {
        WireModel::WHITE
    }
}

// ── Public entry points ────────────────────────────────────────────────────

/// Tessellate one entity into a WireModel.
/// For Text/MText entities this produces one WireModel with all glyph strokes
/// encoded as NaN-separated segments (wire_gpu skips NaN pairs).
/// For Solid3D entities this returns an empty wire; use `tessellate_mesh` instead.
pub fn tessellate(
    document: &CadDocument,
    handle: Handle,
    entity: &EntityType,
    selected: bool,
    entity_color: [f32; 4],
    pattern_length: f32,
    pattern: [f32; 8],
    line_weight_px: f32,
) -> WireModel {
    let color = if selected {
        WireModel::SELECTED
    } else {
        entity_color
    };
    let name = handle.value().to_string();

    // ── Try the truck path first ───────────────────────────────────────────
    if let Some(te) = convert(entity, document) {
        match te.object {
            // ── Text / MText: pre-tessellated glyph strokes ───────────────
            TruckObject::Text(strokes_2d) => {
                // Elevation comes from the entity's Z coordinate.
                let elev = entity_z(entity);

                // Pack all strokes into one flat point list, separated by
                // NaN sentinels so wire_gpu.rs skips disconnected segments.
                let mut points: Vec<[f32; 3]> = Vec::new();
                for (i, stroke) in strokes_2d.iter().enumerate() {
                    if stroke.len() < 2 {
                        continue;
                    }
                    if i > 0 && !points.is_empty() {
                        // NaN sentinel — wire_gpu skips any segment where
                        // either endpoint contains NaN.
                        points.push([f32::NAN, f32::NAN, f32::NAN]);
                    }
                    for &[x, y] in stroke {
                        points.push([x, y, elev]);
                    }
                }

                return WireModel {
                    name,
                    points,
                    color,
                    selected,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts: te.snap_pts,
                    tangent_geoms: te.tangent_geoms,
                    aci: 0,
            key_vertices: te.key_vertices,
            aabb: WireModel::UNBOUNDED_AABB,
                };
            }

            // ── Standard topology objects ─────────────────────────────────
            TruckObject::Point(v) => {
                let result = tessellate_vertex(&v);
                match result {
                    TruckTessResult::Point([x, y, z]) => {
                        let s = 0.1_f32;
                        return WireModel {
                            name,
                            points: vec![
                                [x - s, y, z],
                                [x + s, y, z],
                                [x, y - s, z],
                                [x, y + s, z],
                            ],
                            color,
                            selected,
                            pattern_length: 0.0,
                            pattern: [0.0; 8],
                            line_weight_px: 1.0,
                            snap_pts: te.snap_pts,
                            tangent_geoms: te.tangent_geoms,
                            aci: 0,
            key_vertices: te.key_vertices,
            aabb: WireModel::UNBOUNDED_AABB,
                        };
                    }
                    _ => {}
                }
            }

            TruckObject::Curve(e) => {
                if let TruckTessResult::Lines(points) = tessellate_edge(&e) {
                    return WireModel {
                        name,
                        points,
                        color,
                        selected,
                        pattern_length,
                        pattern,
                        line_weight_px,
                        snap_pts: te.snap_pts,
                        tangent_geoms: te.tangent_geoms,
                        aci: 0,
            key_vertices: te.key_vertices,
            aabb: WireModel::UNBOUNDED_AABB,
                    };
                }
            }

            TruckObject::Contour(w) => {
                if let TruckTessResult::Lines(points) = tessellate_wire(&w) {
                    return WireModel {
                        name,
                        points,
                        color,
                        selected,
                        pattern_length,
                        pattern,
                        line_weight_px,
                        snap_pts: te.snap_pts,
                        tangent_geoms: te.tangent_geoms,
                        aci: 0,
            key_vertices: te.key_vertices,
            aabb: WireModel::UNBOUNDED_AABB,
                    };
                }
            }

            TruckObject::Lines(points) => {
                return WireModel {
                    name,
                    points,
                    color,
                    selected,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts: te.snap_pts,
                    tangent_geoms: te.tangent_geoms,
                    aci: 0,
            key_vertices: te.key_vertices,
            aabb: WireModel::UNBOUNDED_AABB,
                };
            }

            TruckObject::Volume(_) => {
                // Solid3D / Region / Body → handled by tessellate_mesh().
                // As a wire fallback, render the pre-computed edge wires
                // stored in the entity when present (e.g. from SOLVIEW output
                // or when the SAT kernel cannot parse the ACIS data).
                let wire_pts = solid_wire_fallback(entity);
                return WireModel::solid(name, wire_pts, color, selected);
            }
        }
    }

    // ── Legacy fallback for Viewport and other unhandled types ────────────
    let (points, snap_pts, tangent_geoms, key_vertices) = legacy_geometry(entity);
    WireModel {
        name,
        points,
        color,
        selected,
        aci: 0,
        pattern_length,
        pattern,
        line_weight_px,
        snap_pts,
        tangent_geoms,
        key_vertices,
        aabb: WireModel::UNBOUNDED_AABB,
    }
}

pub fn tessellate_dimension(
    document: &CadDocument,
    handle: Handle,
    dim: &Dimension,
    selected: bool,
    entity_color: [f32; 4],
    line_weight_px: f32,
) -> Vec<WireModel> {
    let color = if selected {
        WireModel::SELECTED
    } else {
        entity_color
    };
    let name = handle.value().to_string();
    let points = dimension_geometry(dim);
    let key_vertices = points
        .iter()
        .copied()
        .filter(|p| !(p[0].is_nan() || p[1].is_nan() || p[2].is_nan()))
        .collect();

    let mut wires = vec![WireModel {
        name: name.clone(),
        points,
        color,
        selected,
        aci: 0,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px,
        snap_pts: vec![],
        tangent_geoms: vec![],
        key_vertices,
        aabb: WireModel::UNBOUNDED_AABB,
    }];

    if let Some(text) = dimension_text_entity(dim) {
        let mut wire = tessellate(
            document,
            handle,
            &EntityType::Text(text),
            selected,
            entity_color,
            0.0,
            [0.0; 8],
            line_weight_px,
        );
        wire.name = name;
        wires.push(wire);
    }

    wires
}

/// Kept for backwards compatibility — geometry now lives in entities/leader.rs.
#[allow(dead_code)]
fn tessellate_leader(
    handle: Handle,
    leader: &Leader,
    selected: bool,
    entity_color: [f32; 4],
    line_weight_px: f32,
) -> Vec<WireModel> {
    let color = if selected { WireModel::SELECTED } else { entity_color };
    let name = handle.value().to_string();

    let verts = &leader.vertices;
    if verts.len() < 2 {
        return vec![WireModel {
            name,
            points: vec![],
            color,
            selected,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px,
            snap_pts: vec![],
            tangent_geoms: vec![],
            aci: 0,
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
        }];
    }

    let to_f32 = |v: &Vector3| -> [f32; 3] { [v.x as f32, v.y as f32, v.z as f32] };
    let nan = [f32::NAN; 3];

    // Main path
    let mut points: Vec<[f32; 3]> = verts.iter().map(to_f32).collect();

    // Arrowhead at vertex[0] — only when arrow_enabled
    if leader.arrow_enabled {
        let tip = verts[0];
        let next = verts[1];
        let dx = (next.x - tip.x) as f32;
        let dy = (next.y - tip.y) as f32;
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (dx, dy) = (dx / len, dy / len);
        let arrow_size = (leader.text_height as f32).max(1.0) * 0.8;
        let angle = std::f32::consts::PI / 6.0;
        let (s, c) = angle.sin_cos();
        let wing1 = [
            tip.x as f32 + (dx * c - dy * s) * arrow_size,
            tip.y as f32 + (dx * s + dy * c) * arrow_size,
            tip.z as f32,
        ];
        let wing2 = [
            tip.x as f32 + (dx * c + dy * s) * arrow_size,
            tip.y as f32 + (-dx * s + dy * c) * arrow_size,
            tip.z as f32,
        ];
        points.push(nan);
        points.push(wing1);
        points.push(to_f32(&tip));
        points.push(wing2);
    }

    // Landing line at last vertex
    if leader.hookline_enabled {
        let last = *verts.last().unwrap();
        let prev = verts[verts.len() - 2];
        let last_dir_x = (last.x - prev.x) as f32;
        let sign = if last_dir_x >= 0.0 { 1.0_f32 } else { -1.0_f32 };
        let landing_len = leader.text_height as f32 * 1.5;
        let landing_pt = [
            last.x as f32 + sign * landing_len,
            last.y as f32,
            last.z as f32,
        ];
        points.push(nan);
        points.push(to_f32(&last));
        points.push(landing_pt);
    }

    let key_vertices: Vec<[f32; 3]> = verts.iter().map(to_f32).collect();

    vec![WireModel {
        name,
        points,
        color,
        selected,
        aci: 0,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px,
        snap_pts: vec![],
        tangent_geoms: vec![],
        key_vertices,
        aabb: WireModel::UNBOUNDED_AABB,
    }]
}

/// Kept for backwards compatibility — geometry now lives in entities/multileader.rs.
#[allow(dead_code)]
fn tessellate_multileader(
    document: &CadDocument,
    handle: Handle,
    ml: &MultiLeader,
    selected: bool,
    entity_color: [f32; 4],
    line_weight_px: f32,
) -> Vec<WireModel> {
    let color = if selected { WireModel::SELECTED } else { entity_color };
    let name = handle.value().to_string();
    let nan = [f32::NAN; 3];

    let to_f32 = |v: &acadrust::types::Vector3| -> [f32; 3] {
        [v.x as f32, v.y as f32, v.z as f32]
    };

    let arrow_size = ml.arrowhead_size as f32;
    let draw_arrow = arrow_size > 0.0;
    let invisible = ml.path_type == MultiLeaderPathType::Invisible;

    let mut points: Vec<[f32; 3]> = Vec::new();
    let mut key_verts: Vec<[f32; 3]> = Vec::new();
    let mut first_segment = true;

    for root in &ml.context.leader_roots {
        let cp = &root.connection_point;
        let cp_f = to_f32(cp);

        for line in &root.lines {
            if line.points.is_empty() { continue; }

            // Leader line segments (hidden when path_type = Invisible)
            if !invisible {
                if !first_segment { points.push(nan); }
                first_segment = false;

                for p in &line.points {
                    points.push(to_f32(p));
                    key_verts.push(to_f32(p));
                }

                // Closing segment: last bend point → connection_point
                let last = line.points.last().unwrap();
                let last_f = to_f32(last);
                let dist = ((last_f[0]-cp_f[0]).powi(2) + (last_f[1]-cp_f[1]).powi(2)).sqrt();
                if dist > 1e-9 {
                    points.push(cp_f);
                    key_verts.push(cp_f);
                }
            }

            // Arrowhead — only when arrowhead_size > 0
            if draw_arrow {
                let tip = line.points[0];
                let tip_f = to_f32(&tip);
                let next_dir = if line.points.len() >= 2 { line.points[1] } else { *cp };
                let dx = (next_dir.x - tip.x) as f32;
                let dy = (next_dir.y - tip.y) as f32;
                let dlen = (dx * dx + dy * dy).sqrt().max(1e-9);
                let (dx, dy) = (dx / dlen, dy / dlen);
                let angle = std::f32::consts::PI / 6.0;
                let (s, c) = angle.sin_cos();
                let w1 = [tip_f[0] + (dx*c - dy*s)*arrow_size,
                          tip_f[1] + (dx*s + dy*c)*arrow_size, tip_f[2]];
                let w2 = [tip_f[0] + (dx*c + dy*s)*arrow_size,
                          tip_f[1] + (-dx*s + dy*c)*arrow_size, tip_f[2]];
                points.push(nan);
                points.push(w1);
                points.push(tip_f);
                points.push(w2);
            }
        }

        // Short landing shelf at connection_point — respects enable_landing and enable_dogleg
        if ml.enable_landing && ml.enable_dogleg && ml.dogleg_length > 0.0 {
            let dir = &root.direction;
            let dlen = (dir.x * dir.x + dir.y * dir.y).sqrt().max(1e-9);
            let dl = ml.dogleg_length;
            let end = [
                (cp.x + dir.x / dlen * dl) as f32,
                (cp.y + dir.y / dlen * dl) as f32,
                cp.z as f32,
            ];
            points.push(nan);
            points.push(cp_f);
            points.push(end);
        }
    }

    let mut wires = vec![WireModel {
        name: name.clone(),
        points,
        color,
        selected,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px,
        snap_pts: vec![],
        tangent_geoms: vec![],
        aci: 0,
            key_vertices: key_verts,
            aabb: WireModel::UNBOUNDED_AABB,
    }];

    // Render text content as MText wire
    if ml.content_type == acadrust::entities::LeaderContentType::MText
        && !ml.context.text_string.is_empty()
    {
        let mut mtext = acadrust::entities::MText::new();
        mtext.value = ml.context.text_string.clone();
        mtext.insertion_point = ml.context.text_location;
        mtext.height = if ml.context.text_height > 0.0 {
            ml.context.text_height
        } else {
            ml.text_height
        };
        mtext.common.layer = ml.common.layer.clone();
        let mut w = tessellate(
            document, handle, &EntityType::MText(mtext),
            selected, entity_color, 0.0, [0.0; 8], line_weight_px,
        );
        w.name = name;
        wires.push(w);
    }

    wires
}

/// Tessellate a Solid3D entity into a MeshModel (truck Shell/Solid path).
#[allow(dead_code)]
pub fn tessellate_mesh(
    document: &CadDocument,
    handle: Handle,
    entity: &EntityType,
    selected: bool,
    color: [f32; 4],
) -> Option<MeshModel> {
    let te = convert(entity, document)?;
    let result = match te.object {
        TruckObject::Volume(solid) => tessellate_solid(&solid),
        _ => return None,
    };
    truck_tess::tess_to_mesh_model(
        result,
        handle.value().to_string(),
        if selected { MeshModel::SELECTED } else { color },
        selected,
    )
}

// ── Entity Z helper ───────────────────────────────────────────────────────

/// Extract the Z elevation from a text/mtext entity.
fn entity_z(entity: &EntityType) -> f32 {
    match entity {
        EntityType::Text(t) => t.insertion_point.z as f32,
        EntityType::MText(t) => t.insertion_point.z as f32,
        _ => 0.0,
    }
}

// ── Legacy geometry (Viewport, Hatch outline, unrecognised) ───────────────

type Geometry = (
    Vec<[f32; 3]>,
    Vec<(Vec3, SnapHint)>,
    Vec<TangentGeom>,
    Vec<[f32; 3]>,
);

fn legacy_geometry(entity: &EntityType) -> Geometry {
    match entity {
        EntityType::Viewport(vp) => {
            let cx = vp.center.x as f32;
            let cy = vp.center.y as f32;
            let cz = vp.center.z as f32;
            let hw = (vp.width / 2.0) as f32;
            let hh = (vp.height / 2.0) as f32;
            let pts = vec![
                [cx - hw, cy - hh, cz],
                [cx + hw, cy - hh, cz],
                [cx + hw, cy + hh, cz],
                [cx - hw, cy + hh, cz],
                [cx - hw, cy - hh, cz],
            ];
            (pts, vec![], vec![], vec![])
        }
        EntityType::Hatch(h) => {
            let mut pts: Vec<[f32; 3]> = Vec::new();
            'outer: for path in &h.paths {
                for edge in &path.edges {
                    if let acadrust::entities::BoundaryEdge::Polyline(poly) = edge {
                        for v in &poly.vertices {
                            pts.push([v.x as f32, v.y as f32, 0.0]);
                        }
                        if let Some(first) = pts.first().cloned() {
                            pts.push(first);
                        }
                        break 'outer;
                    }
                }
            }
            if pts.is_empty() {
                pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
            }
            (pts, vec![], vec![], vec![])
        }
        EntityType::Ole2Frame(ole) => {
            // OLE objects carry a bounding rectangle in model space.
            // Render a simple X-through-rectangle placeholder.
            let x0 = ole.upper_left_corner.x as f32;
            let y0 = ole.lower_right_corner.y as f32;
            let x1 = ole.lower_right_corner.x as f32;
            let y1 = ole.upper_left_corner.y as f32;
            let z  = ole.upper_left_corner.z as f32;
            if (x1 - x0).abs() < 1e-6 && (y1 - y0).abs() < 1e-6 {
                // Degenerate / unknown size — show a small cross.
                let s = 0.5_f32;
                return (vec![[-s, 0.0, 0.0], [s, 0.0, 0.0]], vec![], vec![], vec![]);
            }
            let pts = vec![
                // Outer rectangle
                [x0, y0, z], [x1, y0, z], [x1, y0, z], [x1, y1, z],
                [x1, y1, z], [x0, y1, z], [x0, y1, z], [x0, y0, z],
                // Diagonal X
                [x0, y0, z], [x1, y1, z],
                [f32::NAN, f32::NAN, f32::NAN],
                [x1, y0, z], [x0, y1, z],
            ];
            (pts, vec![], vec![], vec![[x0, y0, z], [x1, y1, z]])
        }
        _ => {
            let s = 0.5_f32;
            (vec![[-s, 0.0, 0.0], [s, 0.0, 0.0]], vec![], vec![], vec![])
        }
    }
}

/// Extract pre-computed edge-wire points from Solid3D / Region / Body entities.
///
/// AutoCAD stores explicit wire geometry (from SOLVIEW / 3DPLOT) alongside the
/// ACIS data.  We use this as a visible fallback when the SAT tessellator
/// produces no mesh (e.g. binary SAB data or unsupported geometry).
fn solid_wire_fallback(entity: &EntityType) -> Vec<[f32; 3]> {
    let wires: &[acadrust::entities::Wire] = match entity {
        EntityType::Solid3D(s) => &s.wires,
        EntityType::Region(r)  => &r.wires,
        EntityType::Body(b)    => &b.wires,
        _ => return vec![],
    };

    if wires.is_empty() {
        return vec![];
    }

    let mut pts: Vec<[f32; 3]> = Vec::new();
    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        for (i, v) in wire.points.iter().enumerate() {
            if i > 0 {
                // Connect segments: repeat previous point then add current so
                // the wire renderer draws a continuous polyline per wire.
            }
            pts.push([v.x as f32, v.y as f32, v.z as f32]);
        }
        // NaN sentinel separates distinct wire segments.
        pts.push([f32::NAN, f32::NAN, f32::NAN]);
    }
    pts
}

fn dimension_geometry(dim: &Dimension) -> Vec<[f32; 3]> {
    let mut points = Vec::new();
    match dim {
        Dimension::Aligned(d) => {
            let first = vec3(d.first_point);
            let second = vec3(d.second_point);
            let def = vec3(d.definition_point);
            let axis = normalized_or(second - first, Vec3::X);
            append_linear_dimension(&mut points, first, second, def, axis);
        }
        Dimension::Linear(d) => {
            let first = vec3(d.first_point);
            let second = vec3(d.second_point);
            let def = vec3(d.definition_point);
            let axis = Vec3::new(d.rotation.cos() as f32, d.rotation.sin() as f32, 0.0);
            append_linear_dimension(&mut points, first, second, def, normalized_or(axis, Vec3::X));
        }
        Dimension::Radius(d) => {
            let center = vec3(d.angle_vertex);
            let point = vec3(d.definition_point);
            let text = dimension_text_position(dim);
            add_segment(&mut points, center, point);
            add_segment(&mut points, point, text);
            append_arrow(&mut points, point, normalized_or(center - point, Vec3::X), 0.12);
        }
        Dimension::Diameter(d) => {
            let p1 = vec3(d.angle_vertex);
            let p2 = vec3(d.definition_point);
            add_segment(&mut points, p1, p2);
            append_arrow(&mut points, p1, normalized_or(p2 - p1, Vec3::X), 0.12);
            append_arrow(&mut points, p2, normalized_or(p1 - p2, Vec3::X), 0.12);
        }
        Dimension::Angular2Ln(d) => {
            append_angular_dimension(
                &mut points,
                vec3(d.angle_vertex),
                vec3(d.first_point),
                vec3(d.second_point),
                vec3(d.dimension_arc),
            );
        }
        Dimension::Angular3Pt(d) => {
            append_angular_dimension(
                &mut points,
                vec3(d.angle_vertex),
                vec3(d.first_point),
                vec3(d.second_point),
                vec3(d.definition_point),
            );
        }
        Dimension::Ordinate(d) => {
            add_segment(&mut points, vec3(d.feature_location), vec3(d.definition_point));
            add_segment(&mut points, vec3(d.definition_point), vec3(d.leader_endpoint));
        }
    }
    points
}

fn append_linear_dimension(
    points: &mut Vec<[f32; 3]>,
    first: Vec3,
    second: Vec3,
    def: Vec3,
    axis: Vec3,
) {
    let perp = Vec3::new(-axis.y, axis.x, 0.0);
    let dim_line_pos = def.dot(perp);
    let d1 = first + perp * (dim_line_pos - first.dot(perp));
    let d2 = second + perp * (dim_line_pos - second.dot(perp));
    add_segment(points, first, d1);
    add_segment(points, second, d2);
    add_segment(points, d1, d2);
    append_arrow(points, d1, normalized_or(d2 - d1, axis), 0.12);
    append_arrow(points, d2, normalized_or(d1 - d2, -axis), 0.12);
}

fn append_angular_dimension(
    points: &mut Vec<[f32; 3]>,
    vertex: Vec3,
    first: Vec3,
    second: Vec3,
    arc_point: Vec3,
) {
    add_segment(points, vertex, first);
    add_segment(points, vertex, second);

    let radius = vertex.distance(arc_point);
    if radius <= 1e-6 {
        return;
    }

    let start = (first.y - vertex.y).atan2(first.x - vertex.x);
    let mut end = (second.y - vertex.y).atan2(second.x - vertex.x);
    let mut delta = end - start;
    while delta <= 0.0 {
        delta += std::f32::consts::TAU;
    }
    if delta > std::f32::consts::PI {
        end -= std::f32::consts::TAU;
        delta = end - start;
    }

    let steps = 32;
    let mut arc_pts = Vec::with_capacity((steps + 1) as usize);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = start + delta * t;
        arc_pts.push(vertex + Vec3::new(a.cos() * radius, a.sin() * radius, 0.0));
    }
    add_polyline(points, &arc_pts);

    if arc_pts.len() >= 2 {
        append_arrow(
            points,
            arc_pts[0],
            normalized_or(arc_pts[1] - arc_pts[0], Vec3::X),
            0.1,
        );
        let n = arc_pts.len();
        append_arrow(
            points,
            arc_pts[n - 1],
            normalized_or(arc_pts[n - 2] - arc_pts[n - 1], Vec3::X),
            0.1,
        );
    }
}

fn append_arrow(points: &mut Vec<[f32; 3]>, tip: Vec3, dir: Vec3, size: f32) {
    let dir = normalized_or(dir, Vec3::X) * size;
    let left = rotate(dir, 2.6);
    let right = rotate(dir, -2.6);
    add_segment(points, tip, tip + left);
    add_segment(points, tip, tip + right);
}

fn add_segment(points: &mut Vec<[f32; 3]>, a: Vec3, b: Vec3) {
    if !points.is_empty() {
        points.push([f32::NAN, f32::NAN, f32::NAN]);
    }
    points.push([a.x, a.y, a.z]);
    points.push([b.x, b.y, b.z]);
}

fn add_polyline(points: &mut Vec<[f32; 3]>, polyline: &[Vec3]) {
    if polyline.len() < 2 {
        return;
    }
    if !points.is_empty() {
        points.push([f32::NAN, f32::NAN, f32::NAN]);
    }
    points.extend(polyline.iter().map(|p| [p.x, p.y, p.z]));
}

fn dimension_text_entity(dim: &Dimension) -> Option<Text> {
    let value = dimension_text_value(dim)?;
    let pos = dimension_text_position(dim);
    let base = dim.base();
    let mut text = Text::with_value(value, Vector3::new(pos.x as f64, pos.y as f64, pos.z as f64))
        .with_height(dimension_text_height(dim))
        .with_rotation(base.text_rotation);
    text.style = base.style_name.clone();
    text.common = base.common.clone();
    Some(text)
}

fn dimension_text_value(dim: &Dimension) -> Option<String> {
    let base = dim.base();
    if let Some(user_text) = &base.user_text {
        if !user_text.trim().is_empty() {
            return Some(user_text.clone());
        }
    }
    if !base.text.trim().is_empty() {
        return Some(base.text.clone());
    }
    Some(match dim {
        Dimension::Radius(_) => format!("R{:.4}", dim.measurement()),
        Dimension::Diameter(_) => format!("Ø{:.4}", dim.measurement()),
        Dimension::Angular2Ln(_) | Dimension::Angular3Pt(_) => {
            format!("{:.2}°", dim.measurement())
        }
        _ => format!("{:.4}", dim.measurement()),
    })
}

fn dimension_text_position(dim: &Dimension) -> Vec3 {
    let base = dim.base();
    let pos = vec3(base.text_middle_point);
    if pos.length_squared() > 1e-8 {
        return pos;
    }
    match dim {
        Dimension::Aligned(d) => (vec3(d.first_point) + vec3(d.second_point)) * 0.5,
        Dimension::Linear(d) => (vec3(d.first_point) + vec3(d.second_point)) * 0.5,
        Dimension::Radius(d) => (vec3(d.angle_vertex) + vec3(d.definition_point)) * 0.5,
        Dimension::Diameter(d) => (vec3(d.angle_vertex) + vec3(d.definition_point)) * 0.5,
        Dimension::Angular2Ln(d) => vec3(d.dimension_arc),
        Dimension::Angular3Pt(d) => vec3(d.definition_point),
        Dimension::Ordinate(d) => vec3(d.leader_endpoint),
    }
}

fn dimension_text_height(dim: &Dimension) -> f64 {
    let scale = (dim.measurement().abs() * 0.12).clamp(0.25, 2.0);
    if scale.is_finite() { scale } else { 0.25 }
}

fn vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn normalized_or(v: Vec3, fallback: Vec3) -> Vec3 {
    if v.length_squared() <= 1e-12 {
        fallback
    } else {
        v.normalize()
    }
}

fn rotate(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}
