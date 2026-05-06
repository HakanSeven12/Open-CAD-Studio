// DIST command — measure distance and angle between two picked points.

use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;

pub struct DistCommand {
    first: Option<Vec3>,
}

impl DistCommand {
    pub fn new() -> Self {
        Self { first: None }
    }
}

impl CadCommand for DistCommand {
    fn name(&self) -> &'static str {
        "DIST"
    }

    fn prompt(&self) -> String {
        if self.first.is_none() {
            "DIST  Specify first point:".into()
        } else {
            "DIST  Specify second point:".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(p1) = self.first {
            let delta = pt - p1;
            let dist = delta.length();
            let dx = delta.x;
            let dy = delta.z; // Y-up world: Z is the "drawing Y"
            let dz = delta.y; // Y-up world: Y is elevation

            // Angle in XY plane (XZ in world coords) — degrees from +X
            let angle_xy = dy.atan2(dx).to_degrees();
            // Angle from XY plane toward Z (elevation angle)
            let dist_xy = dx.hypot(dy);
            let angle_z = dz.atan2(dist_xy).to_degrees();

            let msg = format!(
                "Distance = {dist:.4},  Angle in XY Plane = {angle_xy:.4}°,  Angle from XY Plane = {angle_z:.4}°\n  Delta X = {dx:.4},  Delta Y = {dy:.4},  Delta Z = {dz:.4}",
            );
            CmdResult::Measurement(msg)
        } else {
            self.first = Some(pt);
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        let p1 = self.first?;
        Some(WireModel {
            name: "dist_preview".into(),
            points: vec![[p1.x, p1.y, p1.z], [pt.x, pt.y, pt.z]],
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
