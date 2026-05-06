// ZOOM WINDOW command — pick two corners to define the zoom area.

use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;

pub struct ZoomWindowCommand {
    first: Option<Vec3>,
}

impl ZoomWindowCommand {
    pub fn new() -> Self {
        Self { first: None }
    }
}

impl CadCommand for ZoomWindowCommand {
    fn name(&self) -> &'static str {
        "ZOOM WINDOW"
    }

    fn prompt(&self) -> String {
        if self.first.is_none() {
            "ZOOM WINDOW  Specify first corner:".into()
        } else {
            "ZOOM WINDOW  Specify opposite corner:".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(p1) = self.first {
            CmdResult::ZoomToWindow { p1, p2: pt }
        } else {
            self.first = Some(pt);
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
        let p1 = self.first?;
        let min = p1.min(pt);
        let max = p1.max(pt);
        // Draw a rectangle preview
        Some(WireModel {
            name: "zoom_window_preview".into(),
            points: vec![
                [min.x, min.y, 0.0],
                [max.x, min.y, 0.0],
                [max.x, max.y, 0.0],
                [min.x, max.y, 0.0],
                [min.x, min.y, 0.0],
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
