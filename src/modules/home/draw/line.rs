// Line tool — ribbon definition + interactive command.
//
// Command:  LINE — H7CAD behaviour:
//   1. First click  → stores start point, prompts for next point
//   2. Each further click → immediately commits an acadrust::Line entity
//      (start→end) to the document; end becomes the new start point
//   3. Enter / Escape → ends the command

use acadrust::types::Vector3;
use acadrust::{EntityType, Line};

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;
use glam::Vec3;

// ── Ribbon definition ─────────────────────────────────────────────────────

pub fn tool() -> ToolDef {
    ToolDef {
        id: "LINE",
        label: "Line",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/line.svg")),
        event: ModuleEvent::Command("LINE".to_string()),
    }
}

// ── Command implementation ────────────────────────────────────────────────

pub struct LineCommand {
    /// The last committed point (start of the next segment).
    last: Option<Vec3>,
}

impl LineCommand {
    pub fn new() -> Self {
        Self { last: None }
    }
}

impl CadCommand for LineCommand {
    fn name(&self) -> &'static str {
        "LINE"
    }

    fn prompt(&self) -> String {
        if self.last.is_none() {
            "LINE  Specify first point:".to_string()
        } else {
            "LINE  Specify next point  [Enter/Esc = done]:".to_string()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(last) = self.last {
            let line = Line::from_points(
                Vector3::new(last.x as f64, last.y as f64, last.z as f64),
                Vector3::new(pt.x as f64, pt.y as f64, pt.z as f64),
            );
            self.last = Some(pt);
            CmdResult::CommitEntity(EntityType::Line(line))
        } else {
            self.last = Some(pt);
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_escape(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        let last = self.last?;
        Some(WireModel {
            name: "rubber_band".to_string(),
            points: vec![[last.x, last.y, last.z], [pt.x, pt.y, pt.z]],
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
