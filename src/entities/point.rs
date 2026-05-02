use acadrust::entities::Point;
use glam::Vec3;
use truck_modeling::{builder, Point3};

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, parse_f64, square_grip};
use crate::entities::traits::{Grippable, PropertyEditable, Transformable, TruckConvertible};
use crate::scene::acad_to_truck::{TruckEntity, TruckObject};
use crate::scene::object::{GripApply, GripDef, PropSection};
use crate::scene::wire_model::SnapHint;

fn to_truck(pt: &Point) -> TruckEntity {
    let normal = (pt.normal.x, pt.normal.y, pt.normal.z);
    let (wx, wy, wz) = crate::scene::transform::ocs_point_to_wcs(
        (pt.location.x, pt.location.y, pt.location.z),
        normal,
    );
    let p = Point3::new(wx, wy, wz);
    let snap = Vec3::new(wx as f32, wy as f32, wz as f32);
    TruckEntity {
        object: TruckObject::Point(builder::vertex(p)),
        snap_pts: vec![(snap, SnapHint::Node)],
        tangent_geoms: vec![],
        key_vertices: vec![],
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
    fn to_truck(&self, _document: &acadrust::CadDocument) -> Option<TruckEntity> {
        Some(to_truck(self))
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
