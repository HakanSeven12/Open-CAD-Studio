// PLOTWINDOW command — pick two corners to define the plot window area.
//
// Works only in paper space layouts. After picking P1 and P2, writes the
// window to the layout's PlotSettings (PlotType::Window).

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;
use glam::Vec3;

pub struct PlotWindowCommand {
    p1: Option<Vec3>,
}

impl PlotWindowCommand {
    pub fn new() -> Self {
        Self { p1: None }
    }
}

impl CadCommand for PlotWindowCommand {
    fn name(&self) -> &'static str { "PLOTWINDOW" }

    fn prompt(&self) -> String {
        if self.p1.is_none() {
            "PLOTWINDOW  Specify first corner of plot window:".into()
        } else {
            "PLOTWINDOW  Specify opposite corner:".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.p1 {
            None => {
                self.p1 = Some(pt);
                CmdResult::NeedPoint
            }
            Some(p1) => CmdResult::SetPlotWindow { p1, p2: pt },
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        let p1 = self.p1?;
        // Draw the selection rectangle.
        Some(WireModel {
            name: "plotwindow_preview".into(),
            points: vec![
                [p1.x, p1.y, p1.z], [pt.x, p1.y, p1.z],
                [pt.x, p1.y, p1.z], [pt.x, pt.y, pt.z],
                [pt.x, pt.y, pt.z], [p1.x, pt.y, pt.z],
                [p1.x, pt.y, pt.z], [p1.x, p1.y, p1.z],
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
