use acadrust::entities::Insert;
use acadrust::types::{Transform, Vector3};
use acadrust::{EntityType, Handle};
use glam::Vec3;

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, parse_f64, ro_prop as ro, square_grip};
use crate::entities::traits::{Grippable, PropertyEditable, Transformable};
use crate::scene::object::{GripApply, GripDef, PropSection};
use crate::scene::{block_cache, render, tessellate};
use crate::scene::wire_model::WireModel;

fn grips(ins: &Insert) -> Vec<GripDef> {
    let p = Vec3::new(
        ins.insert_point.x as f32,
        ins.insert_point.y as f32,
        ins.insert_point.z as f32,
    );
    vec![square_grip(0, p)]
}

fn properties(ins: &Insert) -> PropSection {
    PropSection {
        title: "Geometry".into(),
        props: vec![
            edit("Insert X", "ins_x", ins.insert_point.x),
            edit("Insert Y", "ins_y", ins.insert_point.y),
            edit("Insert Z", "ins_z", ins.insert_point.z),
            edit("Scale X", "x_scale", ins.x_scale()),
            edit("Scale Y", "y_scale", ins.y_scale()),
            edit("Scale Z", "z_scale", ins.z_scale()),
            edit("Rotation", "rotation", ins.rotation.to_degrees()),
            ro("Block", "block", ins.block_name.clone()),
        ],
    }
}

fn apply_geom_prop(ins: &mut Insert, field: &str, value: &str) {
    let Some(v) = parse_f64(value) else {
        return;
    };
    match field {
        "ins_x" => ins.insert_point.x = v,
        "ins_y" => ins.insert_point.y = v,
        "ins_z" => ins.insert_point.z = v,
        "x_scale" => ins.set_x_scale(v),
        "y_scale" => ins.set_y_scale(v),
        "z_scale" => ins.set_z_scale(v),
        "rotation" => ins.rotation = v.to_radians(),
        _ => {}
    }
}

fn apply_grip(ins: &mut Insert, _grip_id: usize, apply: GripApply) {
    match apply {
        GripApply::Absolute(p) => {
            ins.insert_point.x = p.x as f64;
            ins.insert_point.y = p.y as f64;
            ins.insert_point.z = p.z as f64;
        }
        GripApply::Translate(d) => {
            ins.insert_point.x += d.x as f64;
            ins.insert_point.y += d.y as f64;
            ins.insert_point.z += d.z as f64;
        }
    }
}

fn apply_transform(ins: &mut Insert, t: &EntityTransform) {
    crate::scene::transform::apply_standard_entity_transform(ins, t, |entity, p1, p2| {
        let dx = (p2.x - p1.x) as f64;
        let dy = (p2.y - p1.y) as f64;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-12 {
            return;
        }

        let ux = dx / len;
        let uy = dy / len;
        let mirror = acadrust::types::Matrix4 {
            m: [
                [2.0 * ux * ux - 1.0, 2.0 * ux * uy, 0.0, 0.0],
                [2.0 * ux * uy, 2.0 * uy * uy - 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        let t = Transform::from_translation(Vector3::new(-(p1.x as f64), -(p1.y as f64), 0.0))
            .then(&Transform::from_matrix(mirror))
            .then(&Transform::from_translation(Vector3::new(
                p1.x as f64,
                p1.y as f64,
                0.0,
            )));
        acadrust::Entity::apply_transform(entity, &t);
    });
}

impl Grippable for Insert {
    fn grips(&self) -> Vec<GripDef> {
        grips(self)
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        apply_grip(self, grip_id, apply);
    }
}

impl PropertyEditable for Insert {
    fn geometry_properties(&self, _text_style_names: &[String]) -> PropSection {
        properties(self)
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        apply_geom_prop(self, field, value);
    }
}

impl Transformable for Insert {
    fn apply_transform(&mut self, t: &EntityTransform) {
        apply_transform(self, t);
    }
}

impl crate::entities::traits::FallbackTess for Insert {
    fn fallback_geometry(&self, world_offset: [f64; 3]) -> crate::scene::tess_util::FallbackGeometry {
        let [ox, oy, oz] = world_offset;
        let ip = Vec3::new(
            (self.insert_point.x - ox) as f32,
            (self.insert_point.y - oy) as f32,
            (self.insert_point.z - oz) as f32,
        );
        let s = 0.1_f32;
        let pts = vec![
            [ip.x - s, ip.y, ip.z],
            [ip.x + s, ip.y, ip.z],
            [ip.x, ip.y - s, ip.z],
            [ip.x, ip.y + s, ip.z],
        ];
        (
            pts,
            vec![(ip, crate::scene::wire_model::SnapHint::Insertion)],
            vec![],
            vec![],
        )
    }
}
pub(crate) fn append_insert_attribute_wires(
    wires: &mut Vec<WireModel>,
    document: &acadrust::CadDocument,
    ins: &acadrust::entities::Insert,
    insert_handle: Handle,
    sel: bool,
    ins_color: [f32; 4],
    ins_pat_len: f32,
    ins_pat: [f32; 8],
    ins_lw_px: f32,
    bg_color: [f32; 4],
    is_xref: bool,
    pslt_factor: f32,
    anno_scale: f32,
    world_offset: [f64; 3],
) {
    if ins.attributes.is_empty() {
        return;
    }
    // ATTMODE (header.attribute_visibility):
    //   0 = Off    — every attribute hidden
    //   1 = Normal — honour per-attribute `invisible` flag (default)
    //   2 = On     — every attribute forced visible, ignoring its flag
    let attmode = document.header.attribute_visibility;
    if attmode == 0 {
        return;
    }
    for attr in &ins.attributes {
        let per_attr_hidden = attr.common.invisible || attr.flags.invisible;
        if attmode == 1 && per_attr_hidden {
            continue;
        }
        let attr_entity = EntityType::AttributeEntity(attr.clone());
        let (sub_color, sub_plen, sub_pat, sub_lw_px, sub_aci) =
            render::render_style_for_block_sub(
                document,
                &attr_entity,
                ins_color,
                ins_pat_len,
                ins_pat,
                ins_lw_px,
            );
        let sub_color = render::adapt_to_bg(sub_color, bg_color);
        let sub_color = if is_xref && !sel {
            block_cache::fade_toward_bg(sub_color, bg_color)
        } else {
            sub_color
        };
        let sub_aabb = crate::scene::entity_aabb(&attr_entity, world_offset);
        let mut attr_wires = tessellate::tessellate(
            document,
            insert_handle,
            &attr_entity,
            sel,
            sub_color,
            sub_plen * pslt_factor,
            sub_pat.map(|v| v * pslt_factor),
            sub_lw_px,
            world_offset,
            anno_scale,
        );
        // Use the INSERT's handle so selection / picking groups attribute
        // text with the parent insert instead of treating it as a stray text.
        for w in &mut attr_wires {
            w.name = insert_handle.value().to_string();
            w.aci = sub_aci;
            w.aabb = sub_aabb;
        }
        wires.extend(attr_wires);
    }
}

