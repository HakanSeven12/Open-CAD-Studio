// DIMTEDIT command — reposition the text of an existing dimension.
//
// Workflow:
//   1. Pick a dimension entity
//   2. Click the new text position (entity data is injected by update.rs after pick)

use acadrust::{EntityType, Handle};
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_tedit.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMTEDIT",
        label: "Dim Text Edit",
        icon: ICON,
        event: ModuleEvent::Command("DIMTEDIT".to_string()),
    }
}

enum Step {
    PickDim,
    PickTextPos { handle: Handle, entity: Option<EntityType> },
}

pub struct DimTeditCommand {
    step: Step,
}

impl DimTeditCommand {
    pub fn new() -> Self {
        Self { step: Step::PickDim }
    }

}

impl CadCommand for DimTeditCommand {
    fn name(&self) -> &'static str { "DIMTEDIT" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::PickDim => "DIMTEDIT  Select dimension:".into(),
            Step::PickTextPos { .. } => "DIMTEDIT  Specify new location for dimension text:".into(),
        }
    }

    fn needs_entity_pick(&self) -> bool {
        matches!(self.step, Step::PickDim)
    }

    fn on_entity_pick(&mut self, handle: Handle, _pt: Vec3) -> CmdResult {
        if handle.is_null() { return CmdResult::NeedPoint; }
        self.step = Step::PickTextPos { handle, entity: None };
        CmdResult::NeedPoint
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Step::PickTextPos { handle, entity } = &mut self.step {
            let h = *handle;
            if let Some(mut ent) = entity.take() {
                if let EntityType::Dimension(ref mut d) = ent {
                    let new_pt = acadrust::types::Vector3::new(pt.x as f64, pt.y as f64, pt.z as f64);
                    d.base_mut().text_middle_point = new_pt;
                    d.base_mut().insertion_point = new_pt;
                }
                return CmdResult::ReplaceEntity(h, vec![ent]);
            }
            // No entity yet — wait for inject
            CmdResult::NeedPoint
        } else {
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if !matches!(self.step, Step::PickTextPos { .. }) {
            return None;
        }
        let d = 0.2_f32;
        Some(WireModel {
            name: "dimtedit_preview".into(),
            points: vec![
                [pt.x - d, pt.y, pt.z - d],
                [pt.x + d, pt.y, pt.z - d],
                [pt.x + d, pt.y, pt.z + d],
                [pt.x - d, pt.y, pt.z + d],
                [pt.x - d, pt.y, pt.z - d],
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

    fn inject_picked_entity(&mut self, entity: EntityType) {
        if let Step::PickTextPos { entity: slot, .. } = &mut self.step {
            *slot = Some(entity);
        }
    }
}
