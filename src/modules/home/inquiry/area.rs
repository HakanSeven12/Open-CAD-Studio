// AREA command — compute area and perimeter of a polygon picked point by point.
// Press Enter to close and calculate.

use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;

pub struct AreaCommand {
    points: Vec<Vec3>,
}

impl AreaCommand {
    pub fn new() -> Self {
        Self { points: vec![] }
    }
}

impl CadCommand for AreaCommand {
    fn name(&self) -> &'static str {
        "AREA"
    }

    fn prompt(&self) -> String {
        if self.points.is_empty() {
            "AREA  Specify first corner point (Enter to cancel):".into()
        } else {
            format!(
                "AREA  Specify next point ({} picked, Enter to calculate):",
                self.points.len()
            )
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        self.points.push(pt);
        CmdResult::NeedPoint
    }

    fn on_enter(&mut self) -> CmdResult {
        if self.points.len() < 3 {
            return CmdResult::Cancel;
        }
        // Shoelace formula in the XZ plane (Y-up world)
        let n = self.points.len();
        let mut area_sum = 0.0f32;
        let mut perimeter = 0.0f32;
        for idx in 0..n {
            let a = self.points[idx];
            let b = self.points[(idx + 1) % n];
            area_sum += a.x * b.z - b.x * a.z;
            perimeter += (b - a).length();
        }
        let area = (area_sum * 0.5).abs();
        let msg = format!("Area = {area:.4},  Perimeter = {perimeter:.4}");
        CmdResult::Measurement(msg)
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if self.points.is_empty() {
            return None;
        }
        let mut pts: Vec<[f32; 3]> = self.points.iter().map(|p| [p.x, p.y, p.z]).collect();
        pts.push([pt.x, pt.y, pt.z]);
        pts.push([self.points[0].x, self.points[0].y, self.points[0].z]);
        Some(WireModel {
            name: "area_preview".into(),
            points: pts,
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
