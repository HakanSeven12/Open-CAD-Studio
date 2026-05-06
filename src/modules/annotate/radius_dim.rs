use acadrust::entities::{Dimension, DimensionRadius};
use acadrust::types::Vector3;
use acadrust::EntityType;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;
use glam::Vec3;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_radius.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMRADIUS",
        label: "Radius",
        icon: ICON,
        event: ModuleEvent::Command("DIMRADIUS".to_string()),
    }
}

enum Step {
    CenterPoint,
    RadiusPoint(Vec3),
    TextPoint { center: Vec3, point: Vec3 },
}

pub struct RadiusDimensionCommand {
    step: Step,
}

impl RadiusDimensionCommand {
    pub fn new() -> Self {
        Self {
            step: Step::CenterPoint,
        }
    }
}

impl CadCommand for RadiusDimensionCommand {
    fn name(&self) -> &'static str {
        "DIMRADIUS"
    }

    fn prompt(&self) -> String {
        match self.step {
            Step::CenterPoint => "DIMRADIUS  Specify center point:".into(),
            Step::RadiusPoint(_) => "DIMRADIUS  Specify radius point:".into(),
            Step::TextPoint { .. } => "DIMRADIUS  Specify dimension line location:".into(),
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            Step::CenterPoint => {
                self.step = Step::RadiusPoint(pt);
                CmdResult::NeedPoint
            }
            Step::RadiusPoint(center) => {
                self.step = Step::TextPoint { center, point: pt };
                CmdResult::NeedPoint
            }
            Step::TextPoint { center, point } => {
                let mut dim = DimensionRadius::new(v3(center), v3(point));
                dim.base.definition_point = v3(point);
                dim.base.text_middle_point = v3(pt);
                dim.base.insertion_point = v3(pt);
                dim.leader_length = point.distance(pt) as f64;
                dim.base.actual_measurement = dim.measurement();
                CmdResult::CommitAndExit(EntityType::Dimension(Dimension::Radius(dim)))
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_escape(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        match self.step {
            Step::CenterPoint => None,
            Step::RadiusPoint(center) => Some(preview_wire(vec![center, pt])),
            Step::TextPoint { center, point } => Some(preview_wire(vec![
                center,
                point,
                Vec3::new(f32::NAN, f32::NAN, f32::NAN),
                point,
                pt,
            ])),
        }
    }
}

fn v3(pt: Vec3) -> Vector3 {
    Vector3::new(pt.x as f64, pt.y as f64, pt.z as f64)
}

fn preview_wire(points: Vec<Vec3>) -> WireModel {
    WireModel {
        name: "dimradius_preview".to_string(),
        points: points.into_iter().map(|p| [p.x, p.y, p.z]).collect(),
        color: WireModel::CYAN,
        selected: false,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px: 1.0,
        snap_pts: vec![],
        tangent_geoms: vec![],
        aci: 0,
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: vec![],
    }
}
