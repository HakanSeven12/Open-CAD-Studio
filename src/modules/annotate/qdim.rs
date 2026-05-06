// QDIM command — quick dimension: select objects then pick placement line.
//
// Workflow:
//   1. Select objects (window/click, Enter to finish selection)
//   2. Pick a point that defines the dimension-line position
//
// Creates one linear dimension per detected endpoint pair on the selected objects.

use acadrust::entities::{Dimension, DimensionLinear};
use acadrust::types::Vector3;
use acadrust::{EntityType, Handle};
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/qdim.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "QDIM",
        label: "Quick Dim",
        icon: ICON,
        event: ModuleEvent::Command("QDIM".to_string()),
    }
}

enum Step {
    Gathering,
    PlaceLine { handles: Vec<Handle> },
}

pub struct QdimCommand {
    step: Step,
}

impl QdimCommand {
    pub fn new() -> Self {
        Self { step: Step::Gathering }
    }


}

impl CadCommand for QdimCommand {
    fn name(&self) -> &'static str { "QDIM" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::Gathering => "QDIM  Select geometry to dimension (Enter when done):".into(),
            Step::PlaceLine { .. } => "QDIM  Specify dimension line position:".into(),
        }
    }

    fn is_selection_gathering(&self) -> bool {
        matches!(self.step, Step::Gathering)
    }

    fn on_selection_complete(&mut self, handles: Vec<Handle>) -> CmdResult {
        if handles.is_empty() {
            return CmdResult::Cancel;
        }
        self.step = Step::PlaceLine { handles };
        CmdResult::NeedPoint
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Step::PlaceLine { handles } = &self.step {
            CmdResult::Relaunch(
                format!("QDIM_PLACE {:.6} {:.6} {:.6}", pt.x, pt.y, pt.z),
                handles.clone(),
            )
        } else {
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if !matches!(self.step, Step::PlaceLine { .. }) {
            return None;
        }
        let d = 0.5_f32;
        Some(WireModel {
            name: "qdim_preview".into(),
            points: vec![
                [pt.x - d * 3.0, pt.y, pt.z],
                [pt.x + d * 3.0, pt.y, pt.z],
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
        })
    }
}

/// Build a linear dimension between two points at the given dimension-line position.
#[allow(dead_code)]
pub fn make_linear_dim(p1: Vec3, p2: Vec3, dim_pt: Vec3) -> EntityType {
    let v = |p: Vec3| Vector3::new(p.x as f64, p.y as f64, p.z as f64);
    let mut dim = DimensionLinear::new(v(p1), v(p2));
    // Determine axis: horizontal if Δx > Δz, else vertical
    let dx = (p2.x - p1.x).abs();
    let dz = (p2.z - p1.z).abs();
    dim.rotation = if dz > dx { std::f64::consts::FRAC_PI_2 } else { 0.0 };
    dim.definition_point = v(dim_pt);
    dim.base.definition_point = v(dim_pt);
    let text_pos = (p1 + p2) * 0.5;
    dim.base.text_middle_point = v(text_pos);
    dim.base.insertion_point = v(text_pos);
    dim.base.actual_measurement = dim.measurement();
    EntityType::Dimension(Dimension::Linear(dim))
}
