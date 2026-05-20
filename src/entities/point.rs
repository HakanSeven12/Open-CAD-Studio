use acadrust::entities::Point;
use glam::Vec3;
use truck_modeling::{builder, Point3};

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, parse_f64, square_grip};
use crate::entities::traits::{Grippable, PropertyEditable, Transformable, TruckConvertible};
use crate::scene::acad_to_truck::{TruckEntity, TruckObject};
use crate::scene::object::{GripApply, GripDef, PropSection};
use crate::scene::wire_model::SnapHint;

/// Resolve PDSIZE (negative = % of viewport, 0 = 5% default, positive = world).
/// We don't know the viewport height at tessellation time, so percentages and
/// the 0 default expand to a small fixed world size as a best-effort fallback.
fn pdsize_world(pdsize: f64) -> f64 {
    if pdsize > 0.0 {
        pdsize
    } else {
        // Both PDSIZE = 0 and negative (relative) fall back to a sensible
        // visible default until viewport-aware sizing is wired up.
        2.0
    }
}

fn point_glyph(cx: f64, cy: f64, z: f64, pdmode: i16, pdsize: f64) -> Vec<[f64; 3]> {
    // PDMODE bits:
    //   shape:  0=dot, 1=nothing, 2='+', 3='×', 4='|'
    //   +32   = enclose in a circle
    //   +64   = enclose in a square
    //   (+96 = both)
    let shape = (pdmode & 0x0F) as i32;
    let circle = (pdmode & 32) != 0;
    let square = (pdmode & 64) != 0;
    let s = pdsize_world(pdsize) * 0.5;
    let nan = [f64::NAN, f64::NAN, f64::NAN];
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut push_seg = |a: [f64; 3], b: [f64; 3]| {
        if !pts.is_empty() {
            pts.push(nan);
        }
        pts.push(a);
        pts.push(b);
    };
    match shape {
        // 0 = single dot — emit a tiny "+" so it's visible at any zoom.
        0 => {
            let d = s * 0.05;
            push_seg([cx - d, cy, z], [cx + d, cy, z]);
            push_seg([cx, cy - d, z], [cx, cy + d, z]);
        }
        1 => {} // explicit nothing
        2 => {
            push_seg([cx - s, cy, z], [cx + s, cy, z]);
            push_seg([cx, cy - s, z], [cx, cy + s, z]);
        }
        3 => {
            push_seg([cx - s, cy - s, z], [cx + s, cy + s, z]);
            push_seg([cx - s, cy + s, z], [cx + s, cy - s, z]);
        }
        4 => {
            push_seg([cx, cy - s, z], [cx, cy + s, z]);
        }
        _ => {
            push_seg([cx - s, cy, z], [cx + s, cy, z]);
            push_seg([cx, cy - s, z], [cx, cy + s, z]);
        }
    }
    if circle {
        // 16-segment polyline circle.
        const N: usize = 16;
        let mut ring: Vec<[f64; 3]> = Vec::with_capacity(N + 1);
        for i in 0..=N {
            let a = i as f64 * std::f64::consts::TAU / N as f64;
            ring.push([cx + a.cos() * s, cy + a.sin() * s, z]);
        }
        if !pts.is_empty() {
            pts.push(nan);
        }
        pts.extend(ring);
    }
    if square {
        let p1 = [cx - s, cy - s, z];
        let p2 = [cx + s, cy - s, z];
        let p3 = [cx + s, cy + s, z];
        let p4 = [cx - s, cy + s, z];
        if !pts.is_empty() {
            pts.push(nan);
        }
        pts.extend_from_slice(&[p1, p2, p3, p4, p1]);
    }
    pts
}

fn to_truck(pt: &Point, document: &acadrust::CadDocument) -> TruckEntity {
    let normal = (pt.normal.x, pt.normal.y, pt.normal.z);
    let (wx, wy, wz) = crate::scene::transform::ocs_point_to_wcs(
        (pt.location.x, pt.location.y, pt.location.z),
        normal,
    );
    let snap = Vec3::new(wx as f32, wy as f32, wz as f32);
    let pdmode = document.header.point_display_mode;
    let pdsize = document.header.point_display_size;
    if pdmode == 0 {
        // Default: a single vertex (driver handles the dot pixel).
        let p = Point3::new(wx, wy, wz);
        return TruckEntity {
            object: TruckObject::Point(builder::vertex(p)),
            snap_pts: vec![(snap, SnapHint::Node)],
            tangent_geoms: vec![],
            key_vertices: vec![],
            fill_tris: vec![],
        };
    }
    let pts = point_glyph(wx, wy, wz, pdmode, pdsize);
    if pts.is_empty() {
        // PDMODE 1 = nothing — emit an empty Lines wire so picking still works.
        return TruckEntity {
            object: TruckObject::Lines(vec![]),
            snap_pts: vec![(snap, SnapHint::Node)],
            tangent_geoms: vec![],
            key_vertices: vec![[wx, wy, wz]],
            fill_tris: vec![],
        };
    }
    TruckEntity {
        object: TruckObject::Lines(pts),
        snap_pts: vec![(snap, SnapHint::Node)],
        tangent_geoms: vec![],
        key_vertices: vec![[wx, wy, wz]],
        fill_tris: vec![],
    }
}

fn grips(pt: &Point) -> Vec<GripDef> {
    let p = Vec3::new(
        pt.location.x as f32,
        pt.location.y as f32,
        pt.location.z as f32,
    );
    vec![square_grip(0, p)]
}

fn properties(pt: &Point) -> PropSection {
    PropSection {
        title: "Geometry".into(),
        props: vec![
            edit("X", "loc_x", pt.location.x),
            edit("Y", "loc_y", pt.location.y),
            edit("Z", "loc_z", pt.location.z),
        ],
    }
}

fn apply_geom_prop(pt: &mut Point, field: &str, value: &str) {
    let Some(v) = parse_f64(value) else {
        return;
    };
    match field {
        "loc_x" => pt.location.x = v,
        "loc_y" => pt.location.y = v,
        "loc_z" => pt.location.z = v,
        _ => {}
    }
}

fn apply_grip(pt: &mut Point, _grip_id: usize, apply: GripApply) {
    match apply {
        GripApply::Absolute(p) => {
            pt.location.x = p.x as f64;
            pt.location.y = p.y as f64;
            pt.location.z = p.z as f64;
        }
        GripApply::Translate(d) => {
            pt.location.x += d.x as f64;
            pt.location.y += d.y as f64;
            pt.location.z += d.z as f64;
        }
    }
}

fn apply_transform(pt: &mut Point, t: &EntityTransform) {
    crate::scene::transform::apply_standard_entity_transform(pt, t, |entity, p1, p2| {
        crate::scene::transform::reflect_xy_point(
            &mut entity.location.x,
            &mut entity.location.y,
            p1,
            p2,
        );
    });
}

impl TruckConvertible for Point {
    fn to_truck(&self, document: &acadrust::CadDocument) -> Option<TruckEntity> {
        Some(to_truck(self, document))
    }
}

impl Grippable for Point {
    fn grips(&self) -> Vec<GripDef> {
        grips(self)
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        apply_grip(self, grip_id, apply);
    }
}

impl PropertyEditable for Point {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        properties(self)
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        apply_geom_prop(self, field, value);
    }
}

impl Transformable for Point {
    fn apply_transform(&mut self, t: &EntityTransform) {
        apply_transform(self, t);
    }
}
