use std::f64::consts::TAU;

use acadrust::entities::{Polyline, Polyline2D, Polyline3D};
use glam::Vec3;
use truck_modeling::{builder, Edge, Point3, Wire};

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, ro_prop as ro, square_grip};
use crate::entities::traits::{Grippable, PropertyEditable, Transformable, TruckConvertible};
use crate::scene::acad_to_truck::{TruckEntity, TruckObject};
use crate::scene::object::{GripApply, GripDef, PropSection, PropValue, Property};
use crate::scene::wire_model::TangentGeom;

// ── Polyline (old-style 3D heavy polyline) ────────────────────────────────────

fn tessellate_polyline(pl: &Polyline) -> TruckEntity {
    let pts: Vec<[f32; 3]> = pl
        .vertices
        .iter()
        .map(|v| [v.location.x as f32, v.location.y as f32, v.location.z as f32])
        .collect();

    let mut points = pts.clone();
    if pl.flags.is_closed() && pts.len() >= 2 {
        points.push(pts[0]);
    }

    let key_verts = pts.clone();
    TruckEntity {
        object: TruckObject::Lines(points),
        snap_pts: vec![],
        tangent_geoms: vec![],
        key_vertices: key_verts,
    }
}

impl TruckConvertible for Polyline {
    fn to_truck(&self, _document: &acadrust::CadDocument) -> Option<TruckEntity> {
        Some(tessellate_polyline(self))
    }
}

impl Grippable for Polyline {
    fn grips(&self) -> Vec<GripDef> {
        self.vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                square_grip(
                    i,
                    Vec3::new(v.location.x as f32, v.location.y as f32, v.location.z as f32),
                )
            })
            .collect()
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        if let Some(v) = self.vertices.get_mut(grip_id) {
            match apply {
                GripApply::Translate(d) => {
                    v.location.x += d.x as f64;
                    v.location.y += d.y as f64;
                    v.location.z += d.z as f64;
                }
                GripApply::Absolute(p) => {
                    v.location.x = p.x as f64;
                    v.location.y = p.y as f64;
                    v.location.z = p.z as f64;
                }
            }
        }
    }
}

impl PropertyEditable for Polyline {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        PropSection {
            title: "Geometry".into(),
            props: vec![
                ro("Vertices", "vertices", self.vertices.len().to_string()),
                Property {
                    label: "Closed".into(),
                    field: "pl_closed",
                    value: PropValue::BoolToggle {
                        field: "pl_closed",
                        value: self.flags.is_closed(),
                    },
                },
            ],
        }
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        if field == "pl_closed" {
            let closed = if value == "toggle" {
                !self.flags.is_closed()
            } else {
                value == "true"
            };
            self.flags.set_closed(closed);
        }
    }
}

impl Transformable for Polyline {
    fn apply_transform(&mut self, t: &EntityTransform) {
        crate::scene::transform::apply_standard_entity_transform(self, t, |entity, p1, p2| {
            for v in &mut entity.vertices {
                crate::scene::transform::reflect_xy_point(
                    &mut v.location.x,
                    &mut v.location.y,
                    p1,
                    p2,
                );
            }
        });
    }
}

// ── Polyline2D (heavy 2D polyline with bulge) ─────────────────────────────────

fn tessellate_polyline2d(pl: &Polyline2D) -> TruckEntity {
    let verts = &pl.vertices;
    if verts.is_empty() {
        return TruckEntity {
            object: TruckObject::Lines(vec![]),
            snap_pts: vec![],
            tangent_geoms: vec![],
            key_vertices: vec![],
        };
    }

    let elev = pl.elevation;
    let count = verts.len();
    let seg_count = if pl.is_closed() { count } else { count - 1 };
    let mut edges: Vec<Edge> = Vec::new();
    let mut tangents: Vec<TangentGeom> = Vec::new();
    let mut key_verts: Vec<[f32; 3]> = Vec::new();

    let to_pt = |v: &acadrust::entities::Vertex2D| -> Point3 {
        Point3::new(v.location.x, v.location.y, elev)
    };

    for i in 0..seg_count {
        let v0 = &verts[i];
        let v1 = &verts[(i + 1) % count];
        let p0 = to_pt(v0);
        let p1 = to_pt(v1);
        let bulge = v0.bulge;

        if bulge.abs() < 1e-9 {
            let tv0 = builder::vertex(p0);
            let tv1 = builder::vertex(p1);
            edges.push(builder::line(&tv0, &tv1));
            tangents.push(TangentGeom::Line {
                p1: [p0.x as f32, p0.y as f32, p0.z as f32],
                p2: [p1.x as f32, p1.y as f32, p1.z as f32],
            });
        } else {
            let angle = 4.0 * bulge.atan();
            let dx = p1.x - p0.x;
            let dy = p1.y - p0.y;
            let d = (dx * dx + dy * dy).sqrt();
            let r = (d / 2.0) / (angle / 2.0).sin().abs();
            let mx = (p0.x + p1.x) * 0.5;
            let my = (p0.y + p1.y) * 0.5;
            let len = d.max(1e-12);
            let px = -dy / len;
            let py = dx / len;
            let sagitta_sign = if bulge > 0.0 { 1.0_f64 } else { -1.0_f64 };
            let h = r - (r * r - d * d / 4.0).max(0.0).sqrt();
            let cx = mx + sagitta_sign * px * (r - h);
            let cy = my + sagitta_sign * py * (r - h);
            let mid_a = {
                let a0 = (p0.y - cy).atan2(p0.x - cx);
                let a1 = (p1.y - cy).atan2(p1.x - cx);
                let (sa, mut ea) = if bulge > 0.0 { (a0, a1) } else { (a1, a0) };
                if ea < sa {
                    ea += TAU;
                }
                sa + (ea - sa) * 0.5
            };
            let p_mid = Point3::new(cx + r * mid_a.cos(), cy + r * mid_a.sin(), p0.z);
            let tv0 = builder::vertex(p0);
            let tv1 = builder::vertex(p1);
            edges.push(builder::circle_arc(&tv0, &tv1, p_mid));
            tangents.push(TangentGeom::Circle {
                center: [cx as f32, cy as f32, p0.z as f32],
                radius: r as f32,
            });
        }

        if i == 0 {
            key_verts.push([p0.x as f32, p0.y as f32, p0.z as f32]);
        }
        key_verts.push([p1.x as f32, p1.y as f32, p1.z as f32]);
    }

    TruckEntity {
        object: TruckObject::Contour(edges.into_iter().collect::<Wire>()),
        snap_pts: vec![],
        tangent_geoms: tangents,
        key_vertices: key_verts,
    }
}

impl TruckConvertible for Polyline2D {
    fn to_truck(&self, _document: &acadrust::CadDocument) -> Option<TruckEntity> {
        Some(tessellate_polyline2d(self))
    }
}

impl Grippable for Polyline2D {
    fn grips(&self) -> Vec<GripDef> {
        let elev = self.elevation as f32;
        self.vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                square_grip(
                    i,
                    Vec3::new(v.location.x as f32, v.location.y as f32, elev),
                )
            })
            .collect()
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        if let Some(v) = self.vertices.get_mut(grip_id) {
            match apply {
                GripApply::Translate(d) => {
                    v.location.x += d.x as f64;
                    v.location.y += d.y as f64;
                }
                GripApply::Absolute(p) => {
                    v.location.x = p.x as f64;
                    v.location.y = p.y as f64;
                }
            }
        }
    }
}

impl PropertyEditable for Polyline2D {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        PropSection {
            title: "Geometry".into(),
            props: vec![
                ro("Vertices", "vertices", self.vertices.len().to_string()),
                edit("Elevation", "pl2_elevation", self.elevation),
                Property {
                    label: "Closed".into(),
                    field: "pl2_closed",
                    value: PropValue::BoolToggle {
                        field: "pl2_closed",
                        value: self.is_closed(),
                    },
                },
            ],
        }
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        match field {
            "pl2_closed" => {
                let closed = if value == "toggle" {
                    !self.is_closed()
                } else {
                    value == "true"
                };
                if closed { self.close(); } else { self.flags.set_closed(false); }
            }
            "pl2_elevation" => {
                if let Ok(v) = value.trim().parse::<f64>() {
                    self.elevation = v;
                }
            }
            _ => {}
        }
    }
}

impl Transformable for Polyline2D {
    fn apply_transform(&mut self, t: &EntityTransform) {
        crate::scene::transform::apply_standard_entity_transform(self, t, |entity, p1, p2| {
            for v in &mut entity.vertices {
                crate::scene::transform::reflect_xy_point(
                    &mut v.location.x,
                    &mut v.location.y,
                    p1,
                    p2,
                );
            }
        });
    }
}

// ── Polyline3D ────────────────────────────────────────────────────────────────

fn tessellate_polyline3d(pl: &Polyline3D) -> TruckEntity {
    let to_pt = |v: &acadrust::entities::Vertex3DPolyline| -> [f32; 3] {
        [v.position.x as f32, v.position.y as f32, v.position.z as f32]
    };

    // DXF vertex flags:  8 = spline-fit curve point,  16 = spline frame control point.
    // When spline-fit vertices are present use them for the wire and control points for snap;
    // otherwise treat all vertices uniformly.
    let spline_curve: Vec<_> = pl.vertices.iter().filter(|v| v.flags & 8 != 0).collect();
    let ctrl_pts: Vec<_>     = pl.vertices.iter().filter(|v| v.flags & 16 != 0).collect();

    let (wire_pts, key_verts) = if !spline_curve.is_empty() {
        let wire: Vec<[f32; 3]> = spline_curve.iter().map(|v| to_pt(v)).collect();
        let ctrl: Vec<[f32; 3]> = ctrl_pts.iter().map(|v| to_pt(v)).collect();
        (wire, ctrl)
    } else {
        let pts: Vec<[f32; 3]> = pl.vertices.iter().map(to_pt).collect();
        (pts.clone(), pts)
    };

    let mut points = wire_pts.clone();
    if pl.is_closed() && wire_pts.len() >= 2 {
        points.push(wire_pts[0]);
    }

    TruckEntity {
        object: TruckObject::Lines(points),
        snap_pts: vec![],
        tangent_geoms: vec![],
        key_vertices: key_verts,
    }
}

impl TruckConvertible for Polyline3D {
    fn to_truck(&self, _document: &acadrust::CadDocument) -> Option<TruckEntity> {
        Some(tessellate_polyline3d(self))
    }
}

impl Grippable for Polyline3D {
    fn grips(&self) -> Vec<GripDef> {
        self.vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                square_grip(
                    i,
                    Vec3::new(v.position.x as f32, v.position.y as f32, v.position.z as f32),
                )
            })
            .collect()
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        if let Some(v) = self.vertices.get_mut(grip_id) {
            match apply {
                GripApply::Translate(d) => {
                    v.position.x += d.x as f64;
                    v.position.y += d.y as f64;
                    v.position.z += d.z as f64;
                }
                GripApply::Absolute(p) => {
                    v.position.x = p.x as f64;
                    v.position.y = p.y as f64;
                    v.position.z = p.z as f64;
                }
            }
        }
    }
}

impl PropertyEditable for Polyline3D {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        PropSection {
            title: "Geometry".into(),
            props: vec![
                ro("Vertices", "vertices", self.vertices.len().to_string()),
                Property {
                    label: "Closed".into(),
                    field: "pl3_closed",
                    value: PropValue::BoolToggle {
                        field: "pl3_closed",
                        value: self.is_closed(),
                    },
                },
            ],
        }
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        if field == "pl3_closed" {
            let closed = if value == "toggle" { !self.is_closed() } else { value == "true" };
            if closed { self.close(); } else { self.open(); }
        }
    }
}

impl Transformable for Polyline3D {
    fn apply_transform(&mut self, t: &EntityTransform) {
        crate::scene::transform::apply_standard_entity_transform(self, t, |entity, p1, p2| {
            for v in &mut entity.vertices {
                crate::scene::transform::reflect_xy_point(
                    &mut v.position.x,
                    &mut v.position.y,
                    p1,
                    p2,
                );
            }
        });
    }
}
