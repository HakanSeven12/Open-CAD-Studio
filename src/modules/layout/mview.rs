// MVIEW — interactive paper-space viewport creation.
//
// Two clicks define opposite corners of a new viewport rectangle.
// The created Viewport entity is routed to add_entity_to_layout by apply_cmd_result
// because we're in paper space.

use acadrust::entities::Viewport;
use acadrust::types::Vector3;
use acadrust::EntityType;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;
use glam::Vec3;

// ── Ribbon definition ─────────────────────────────────────────────────────

pub fn tool() -> ToolDef {
    ToolDef {
        id: "MVIEW",
        label: "Viewport",
        icon: IconKind::Glyph("⬜"),
        event: ModuleEvent::Command("MVIEW".to_string()),
    }
}

// ── Command ───────────────────────────────────────────────────────────────

pub struct MviewCommand {
    corner1: Option<Vec3>,
}

impl MviewCommand {
    pub fn new() -> Self {
        Self { corner1: None }
    }
}

impl CadCommand for MviewCommand {
    fn name(&self) -> &'static str {
        "MVIEW"
    }

    fn prompt(&self) -> String {
        if self.corner1.is_none() {
            "MVIEW  İlk köşeyi belirtin:".to_string()
        } else {
            "MVIEW  Karşı köşeyi belirtin:".to_string()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(c1) = self.corner1 {
            let w = (pt.x - c1.x).abs() as f64;
            let h = (pt.y - c1.y).abs() as f64;
            if w < 1.0 || h < 1.0 {
                return CmdResult::Cancel;
            }
            let cx = ((c1.x + pt.x) / 2.0) as f64;
            let cy = ((c1.y + pt.y) / 2.0) as f64;
            let cz = c1.z as f64;

            let mut vp = Viewport::new();
            vp.center = Vector3::new(cx, cy, cz);
            vp.width = w;
            vp.height = h;
            vp.id = 2; // user viewport (id > 1)

            CmdResult::CommitAndExit(EntityType::Viewport(vp))
        } else {
            self.corner1 = Some(pt);
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        let c1 = self.corner1?;
        Some(WireModel {
            name: "mview_preview".to_string(),
            points: vec![
                [c1.x, c1.y, c1.z],
                [pt.x, c1.y, c1.z],
                [pt.x, pt.y, c1.z],
                [c1.x, pt.y, c1.z],
                [c1.x, c1.y, c1.z],
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
        })
    }
}
