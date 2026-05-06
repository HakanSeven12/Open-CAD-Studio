// DIMALIGNED command — aligned dimension (measures true distance between two points).

use acadrust::entities::{Dimension, DimensionAligned};
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_aligned.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMALIGNED",
        label: "Aligned",
        icon: ICON,
        event: ModuleEvent::Command("DIMALIGNED".to_string()),
    }
}

enum Step {
    First,
    Second(Vec3),
    DimLine { p1: Vec3, p2: Vec3 },
}

pub struct AlignedDimensionCommand {
    step: Step,
}

impl AlignedDimensionCommand {
    pub fn new() -> Self {
        Self { step: Step::First }
    }
}

impl CadCommand for AlignedDimensionCommand {
    fn name(&self) -> &'static str { "DIMALIGNED" }

    fn prompt(&self) -> String {
        match self.step {
            Step::First           => "DIMALIGNED  Specify first extension line origin:".into(),
            Step::Second(_)       => "DIMALIGNED  Specify second extension line origin:".into(),
            Step::DimLine { .. }  => "DIMALIGNED  Specify dimension line location:".into(),
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            Step::First => {
                self.step = Step::Second(pt);
                CmdResult::NeedPoint
            }
            Step::Second(p1) => {
                self.step = Step::DimLine { p1, p2: pt };
                CmdResult::NeedPoint
            }
            Step::DimLine { p1, p2 } => {
                let mut dim = DimensionAligned::new(v3(p1), v3(p2));
                // Set offset: distance from p2 to the dim line location
                let dx = pt.x as f64 - p2.x as f64;
                let dy = pt.z as f64 - p2.z as f64;
                let offset = (dx * dx + dy * dy).sqrt();
                dim.set_offset(offset);
                dim.base.definition_point = v3(pt);
                dim.base.text_middle_point = v3(
                    Vec3::new(
                        (p1.x + p2.x) * 0.5,
                        (p1.y + p2.y) * 0.5,
                        (p1.z + p2.z) * 0.5,
                    )
                );
                dim.base.insertion_point = dim.base.text_middle_point;
                dim.base.actual_measurement = dim.measurement();
                CmdResult::CommitAndExit(EntityType::Dimension(Dimension::Aligned(dim)))
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        let (p1, p2) = match self.step {
            Step::First => return None,
            Step::Second(p1) => (p1, pt),
            Step::DimLine { p1, p2 } => {
                return Some(preview_aligned(p1, p2, pt));
            }
        };
        Some(WireModel {
            name: "dimaligned_preview".into(),
            points: vec![[p1.x, p1.y, p1.z], [p2.x, p2.y, p2.z]],
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
        })
    }
}

fn v3(p: Vec3) -> Vector3 {
    Vector3::new(p.x as f64, p.y as f64, p.z as f64)
}

fn preview_aligned(p1: Vec3, p2: Vec3, dim_pt: Vec3) -> WireModel {
    // Show ext lines + dim line
    let dir = (p2 - p1).normalize_or_zero();
    let perp = Vec3::new(-dir.z, dir.y, dir.x).normalize_or_zero();
    let offset = (dim_pt - p2).dot(perp);
    let d1 = p1 + perp * offset;
    let d2 = p2 + perp * offset;
    WireModel {
        name: "dimaligned_preview".into(),
        points: vec![
            [p1.x, p1.y, p1.z], [d1.x, d1.y, d1.z],
            [f32::NAN, 0.0, 0.0],
            [p2.x, p2.y, p2.z], [d2.x, d2.y, d2.z],
            [f32::NAN, 0.0, 0.0],
            [d1.x, d1.y, d1.z], [d2.x, d2.y, d2.z],
        ],
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
