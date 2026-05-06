use acadrust::entities::{AttributeDefinition, AttributeEntity};
use glam::Vec3;

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, ro_prop as ro, square_grip};
use crate::entities::text_support::resolve_text_style;
use crate::entities::traits::{Grippable, PropertyEditable, Transformable, TruckConvertible};
use crate::scene::acad_to_truck::{TextStroke, TruckEntity, TruckObject};
use crate::scene::object::{GripApply, GripDef, PropSection};
use crate::scene::wire_model::SnapHint;
use crate::scene::{cxf, transform};

// ── AttributeDefinition ───────────────────────────────────────────────────────

impl TruckConvertible for AttributeDefinition {
    fn to_truck(&self, document: &acadrust::CadDocument) -> Option<TruckEntity> {
        let normal = (self.normal.x, self.normal.y, self.normal.z);
        let (wsx, wsy, wsz) = transform::ocs_point_to_wcs(
            (self.insertion_point.x, self.insertion_point.y, self.insertion_point.z),
            normal,
        );
        let snap_pt = Vec3::new(wsx as f32, wsy as f32, wsz as f32);
        let resolved = resolve_text_style(&self.text_style, document);
        let display = if self.default_value.is_empty() {
            format!("[{}]", self.tag)
        } else {
            self.default_value.clone()
        };
        let wf = (self.width_factor as f32).max(0.01);
        let origin = [self.insertion_point.x, self.insertion_point.y];
        let strokes = cxf::tessellate_text_ex(
            [0.0, 0.0],
            self.height as f32,
            self.rotation as f32,
            wf * resolved.width_factor.max(0.01),
            self.oblique_angle as f32 + resolved.oblique_angle,
            &resolved.font_name,
            &display,
        );
        Some(TruckEntity {
            object: TruckObject::Text(vec![TextStroke { strokes, origin }]),
            snap_pts: vec![(snap_pt, SnapHint::Insertion)],
            tangent_geoms: vec![],
            key_vertices: vec![],
            fill_tris: vec![],
        })
    }
}

impl Grippable for AttributeDefinition {
    fn grips(&self) -> Vec<GripDef> {
        vec![square_grip(
            0,
            Vec3::new(
                self.insertion_point.x as f32,
                self.insertion_point.y as f32,
                self.insertion_point.z as f32,
            ),
        )]
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        if grip_id == 0 {
            match apply {
                GripApply::Translate(d) => {
                    self.insertion_point.x += d.x as f64;
                    self.insertion_point.y += d.y as f64;
                    self.insertion_point.z += d.z as f64;
                }
                GripApply::Absolute(p) => {
                    self.insertion_point.x = p.x as f64;
                    self.insertion_point.y = p.y as f64;
                    self.insertion_point.z = p.z as f64;
                }
            }
        }
    }
}

impl PropertyEditable for AttributeDefinition {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        PropSection {
            title: "Geometry".into(),
            props: vec![
                ro("Tag", "att_tag", self.tag.clone()),
                ro("Prompt", "att_prompt", self.prompt.clone()),
                edit("Default", "att_default", 0.0), // String — handled as text
                edit("Insert X", "att_ix", self.insertion_point.x),
                edit("Insert Y", "att_iy", self.insertion_point.y),
                edit("Insert Z", "att_iz", self.insertion_point.z),
                edit("Height", "att_h", self.height),
                edit("Rotation", "att_rot", self.rotation.to_degrees()),
            ],
        }
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        let Ok(v) = value.trim().parse::<f64>() else { return };
        match field {
            "att_ix" => self.insertion_point.x = v,
            "att_iy" => self.insertion_point.y = v,
            "att_iz" => self.insertion_point.z = v,
            "att_h" if v > 0.0 => self.height = v,
            "att_rot" => self.rotation = v.to_radians(),
            _ => {}
        }
    }
}

impl Transformable for AttributeDefinition {
    fn apply_transform(&mut self, t: &EntityTransform) {
        transform::apply_standard_entity_transform(self, t, |entity, p1, p2| {
            transform::reflect_xy_point(
                &mut entity.insertion_point.x,
                &mut entity.insertion_point.y,
                p1,
                p2,
            );
        });
    }
}

// ── AttributeEntity ───────────────────────────────────────────────────────────

impl TruckConvertible for AttributeEntity {
    fn to_truck(&self, document: &acadrust::CadDocument) -> Option<TruckEntity> {
        let normal = (self.normal.x, self.normal.y, self.normal.z);
        let (wsx, wsy, wsz) = transform::ocs_point_to_wcs(
            (self.insertion_point.x, self.insertion_point.y, self.insertion_point.z),
            normal,
        );
        let snap_pt = Vec3::new(wsx as f32, wsy as f32, wsz as f32);
        let resolved = resolve_text_style(&self.text_style, document);
        let wf = (self.width_factor as f32).max(0.01);
        let origin = [self.insertion_point.x, self.insertion_point.y];
        let strokes = cxf::tessellate_text_ex(
            [0.0, 0.0],
            self.height as f32,
            self.rotation as f32,
            wf * resolved.width_factor.max(0.01),
            self.oblique_angle as f32 + resolved.oblique_angle,
            &resolved.font_name,
            &self.value,
        );
        Some(TruckEntity {
            object: TruckObject::Text(vec![TextStroke { strokes, origin }]),
            snap_pts: vec![(snap_pt, SnapHint::Insertion)],
            tangent_geoms: vec![],
            key_vertices: vec![],
            fill_tris: vec![],
        })
    }
}

impl Grippable for AttributeEntity {
    fn grips(&self) -> Vec<GripDef> {
        vec![square_grip(
            0,
            Vec3::new(
                self.insertion_point.x as f32,
                self.insertion_point.y as f32,
                self.insertion_point.z as f32,
            ),
        )]
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        if grip_id == 0 {
            match apply {
                GripApply::Translate(d) => {
                    self.insertion_point.x += d.x as f64;
                    self.insertion_point.y += d.y as f64;
                    self.insertion_point.z += d.z as f64;
                }
                GripApply::Absolute(p) => {
                    self.insertion_point.x = p.x as f64;
                    self.insertion_point.y = p.y as f64;
                    self.insertion_point.z = p.z as f64;
                }
            }
        }
    }
}

impl PropertyEditable for AttributeEntity {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        PropSection {
            title: "Geometry".into(),
            props: vec![
                ro("Tag", "atte_tag", self.tag.clone()),
                ro("Value", "atte_val", self.value.clone()),
                edit("Insert X", "atte_ix", self.insertion_point.x),
                edit("Insert Y", "atte_iy", self.insertion_point.y),
                edit("Insert Z", "atte_iz", self.insertion_point.z),
                edit("Height", "atte_h", self.height),
                edit("Rotation", "atte_rot", self.rotation.to_degrees()),
            ],
        }
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        let Ok(v) = value.trim().parse::<f64>() else { return };
        match field {
            "atte_ix" => self.insertion_point.x = v,
            "atte_iy" => self.insertion_point.y = v,
            "atte_iz" => self.insertion_point.z = v,
            "atte_h" if v > 0.0 => self.height = v,
            "atte_rot" => self.rotation = v.to_radians(),
            _ => {}
        }
    }
}

impl Transformable for AttributeEntity {
    fn apply_transform(&mut self, t: &EntityTransform) {
        transform::apply_standard_entity_transform(self, t, |entity, p1, p2| {
            transform::reflect_xy_point(
                &mut entity.insertion_point.x,
                &mut entity.insertion_point.y,
                p1,
                p2,
            );
        });
    }
}
