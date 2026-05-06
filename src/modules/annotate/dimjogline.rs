// DIMJOGLINE command — add a jog (zigzag) symbol to a linear or aligned dimension.
//
// Workflow:
//   1. Pick the dimension
//   2. Click the position on the dimension line where the jog should appear

use acadrust::Handle;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_jog.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMJOGLINE",
        label: "Jog Line",
        icon: ICON,
        event: ModuleEvent::Command("DIMJOGLINE".to_string()),
    }
}

enum Step {
    PickDim,
    PickJogPos { handle: Handle },
}

pub struct DimJogLineCommand {
    step: Step,
}

impl DimJogLineCommand {
    pub fn new() -> Self {
        Self { step: Step::PickDim }
    }
}

impl CadCommand for DimJogLineCommand {
    fn name(&self) -> &'static str { "DIMJOGLINE" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::PickDim => "DIMJOGLINE  Select linear or aligned dimension:".into(),
            Step::PickJogPos { .. } => "DIMJOGLINE  Specify jog location:".into(),
        }
    }

    fn needs_entity_pick(&self) -> bool {
        matches!(self.step, Step::PickDim)
    }

    fn on_entity_pick(&mut self, handle: Handle, _pt: Vec3) -> CmdResult {
        if handle.is_null() { return CmdResult::NeedPoint; }
        self.step = Step::PickJogPos { handle };
        CmdResult::NeedPoint
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Step::PickJogPos { handle } = &self.step {
            let h = *handle;
            // Emit sentinel for commands.rs to store the jog position
            use acadrust::entities::XLine;
            let mut xl = XLine::default();
            xl.common.layer = format!("__DIMJOG__{},{:.6},{:.6}", h.value(), pt.x, pt.z);
            return CmdResult::ReplaceEntity(h, vec![acadrust::EntityType::XLine(xl)]);
        }
        CmdResult::NeedPoint
    }

    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if !matches!(self.step, Step::PickJogPos { .. }) {
            return None;
        }
        let d = 0.3_f32;
        Some(WireModel {
            name: "dimjog_preview".into(),
            points: vec![
                [pt.x - d, pt.y, pt.z],
                [pt.x - d * 0.3, pt.y, pt.z + d],
                [pt.x + d * 0.3, pt.y, pt.z - d],
                [pt.x + d, pt.y, pt.z],
            ],
            color: WireModel::CYAN,
            selected: false,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px: 1.2,
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
