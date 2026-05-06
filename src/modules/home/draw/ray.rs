// Ray / XLine draw commands.
//
//  RAY   — semi-infinite line: click base point, then click direction point.
//          Produces a Ray entity; repeats until Enter/Esc.
//  XLINE — infinite construction line: same two-click pattern, yields XLine.

use acadrust::entities::{Ray as RayEnt, XLine as XLineEnt};
use acadrust::types::Vector3;
use acadrust::EntityType;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;
use glam::Vec3;

const DISPLAY_EXTENT: f32 = 1_000_000.0;

// ── RAY ───────────────────────────────────────────────────────────────────

pub struct RayCommand {
    base: Option<Vec3>,
}

impl RayCommand {
    pub fn new() -> Self {
        Self { base: None }
    }
}

impl CadCommand for RayCommand {
    fn name(&self) -> &'static str {
        "RAY"
    }

    fn prompt(&self) -> String {
        if self.base.is_none() {
            "RAY  Specify start point:".into()
        } else {
            "RAY  Specify through point  [Enter/Esc = done]:".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(base) = self.base {
            let dir = pt - base;
            let len = dir.length();
            if len < 1e-6 {
                return CmdResult::NeedPoint;
            }
            let dir_n = dir / len;
            let ray = RayEnt::new(
                Vector3::new(base.x as f64, base.y as f64, base.z as f64),
                Vector3::new(dir_n.x as f64, dir_n.y as f64, dir_n.z as f64),
            );
            // Stay active: new base = same base (can keep clicking through points)
            // Actually AutoCAD prompts for new start after each ray — reset base.
            self.base = None;
            CmdResult::CommitEntity(EntityType::Ray(ray))
        } else {
            self.base = Some(pt);
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
        let base = self.base?;
        let dir = (pt - base).normalize_or_zero();
        let far = base + dir * DISPLAY_EXTENT;
        Some(WireModel {
            name: "ray_preview".into(),
            points: vec![[base.x, base.y, base.z], [far.x, far.y, far.z]],
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

// ── XLINE ─────────────────────────────────────────────────────────────────

pub struct XLineCommand {
    base: Option<Vec3>,
}

impl XLineCommand {
    pub fn new() -> Self {
        Self { base: None }
    }
}

impl CadCommand for XLineCommand {
    fn name(&self) -> &'static str {
        "XLINE"
    }

    fn prompt(&self) -> String {
        if self.base.is_none() {
            "XLINE  Specify a point:".into()
        } else {
            "XLINE  Specify through point  [Enter/Esc = done]:".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(base) = self.base {
            let dir = pt - base;
            let len = dir.length();
            if len < 1e-6 {
                return CmdResult::NeedPoint;
            }
            let dir_n = dir / len;
            let xline = XLineEnt::new(
                Vector3::new(base.x as f64, base.y as f64, base.z as f64),
                Vector3::new(dir_n.x as f64, dir_n.y as f64, dir_n.z as f64),
            );
            self.base = None;
            CmdResult::CommitEntity(EntityType::XLine(xline))
        } else {
            self.base = Some(pt);
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
        let base = self.base?;
        let dir = (pt - base).normalize_or_zero();
        let far_pos = base + dir * DISPLAY_EXTENT;
        let far_neg = base - dir * DISPLAY_EXTENT;
        Some(WireModel {
            name: "xline_preview".into(),
            points: vec![
                [far_neg.x, far_neg.y, far_neg.z],
                [far_pos.x, far_pos.y, far_pos.z],
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
