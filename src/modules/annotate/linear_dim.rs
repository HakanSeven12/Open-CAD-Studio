use acadrust::entities::{Dimension, DimensionLinear};
use acadrust::types::Vector3;
use acadrust::EntityType;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;
use glam::Vec3;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_linear.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMLINEAR",
        label: "Linear",
        icon: ICON,
        event: ModuleEvent::Command("DIMLINEAR".to_string()),
    }
}

enum Step {
    FirstPoint,
    SecondPoint(Vec3),
    DimensionLine { first: Vec3, second: Vec3 },
}

pub struct LinearDimensionCommand {
    step: Step,
}

impl LinearDimensionCommand {
    pub fn new() -> Self {
        Self {
            step: Step::FirstPoint,
        }
    }
}

impl CadCommand for LinearDimensionCommand {
    fn name(&self) -> &'static str {
        "DIMLINEAR"
    }

    fn prompt(&self) -> String {
        match self.step {
            Step::FirstPoint => "DIMLINEAR  Specify first extension line origin:".into(),
            Step::SecondPoint(_) => "DIMLINEAR  Specify second extension line origin:".into(),
            Step::DimensionLine { .. } => "DIMLINEAR  Specify dimension line location:".into(),
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            Step::FirstPoint => {
                self.step = Step::SecondPoint(pt);
                CmdResult::NeedPoint
            }
            Step::SecondPoint(first) => {
                self.step = Step::DimensionLine { first, second: pt };
                CmdResult::NeedPoint
            }
            Step::DimensionLine { first, second } => {
                let mut dim = DimensionLinear::new(v3(first), v3(second));
                dim.rotation = if (second.y - first.y).abs() > (second.x - first.x).abs() {
                    std::f64::consts::FRAC_PI_2
                } else {
                    0.0
                };
                dim.definition_point = v3(pt);
                dim.base.definition_point = v3(pt);
                dim.base.text_middle_point = v3(linear_text_pos(first, second, pt));
                dim.base.insertion_point = dim.base.text_middle_point;
                dim.base.actual_measurement = dim.measurement();
                CmdResult::CommitAndExit(EntityType::Dimension(Dimension::Linear(dim)))
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
            Step::FirstPoint => None,
            Step::SecondPoint(first) => Some(preview_wire(vec![first, pt])),
            Step::DimensionLine { first, second } => {
                Some(preview_wire(linear_dimension_preview(first, second, pt)))
            }
        }
    }
}

fn v3(pt: Vec3) -> Vector3 {
    Vector3::new(pt.x as f64, pt.y as f64, pt.z as f64)
}

fn preview_wire(points: Vec<Vec3>) -> WireModel {
    WireModel {
        name: "dimlinear_preview".to_string(),
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
    }
}

fn linear_dimension_preview(first: Vec3, second: Vec3, def: Vec3) -> Vec<Vec3> {
    let axis = {
        let d = second - first;
        if d.length_squared() <= 1e-12 {
            Vec3::X
        } else if d.x.abs() >= d.y.abs() {
            Vec3::X
        } else {
            Vec3::Y
        }
    };
    let perp = Vec3::new(-axis.y, axis.x, 0.0);
    let offset = (def - first).dot(perp);
    let d1 = first + perp * offset;
    let d2 = second + perp * offset;
    vec![
        first,
        d1,
        Vec3::new(f32::NAN, f32::NAN, f32::NAN),
        second,
        d2,
        Vec3::new(f32::NAN, f32::NAN, f32::NAN),
        d1,
        d2,
    ]
}

fn linear_text_pos(first: Vec3, second: Vec3, def: Vec3) -> Vec3 {
    let axis = {
        let d = second - first;
        if d.length_squared() <= 1e-12 {
            Vec3::X
        } else if d.x.abs() >= d.y.abs() {
            Vec3::X
        } else {
            Vec3::Y
        }
    };
    let perp = Vec3::new(-axis.y, axis.x, 0.0);
    let offset = (def - first).dot(perp);
    let d1 = first + perp * offset;
    let d2 = second + perp * offset;
    (d1 + d2) * 0.5 + perp * 0.15
}
