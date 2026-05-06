// MLEADER command
//
// Flow:
//   1. CollectPoints — click arrowhead, then bend points; Enter (≥2) to finish
//   2. AskText       — wants_text_input; blank Enter = no text
//   → commit single MultiLeader entity

use acadrust::entities::MultiLeader;
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/mleader.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "MLEADER",
        label: "MLeader",
        icon: ICON,
        event: ModuleEvent::Command("MLEADER".to_string()),
    }
}

enum Step {
    CollectPoints { verts: Vec<Vec3> },
    AskText { verts: Vec<Vec3> },
}

pub struct MLeaderCommand {
    step: Step,
}

impl MLeaderCommand {
    pub fn new() -> Self {
        Self { step: Step::CollectPoints { verts: Vec::new() } }
    }
}

impl CadCommand for MLeaderCommand {
    fn name(&self) -> &'static str { "MLEADER" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::CollectPoints { verts } if verts.is_empty() =>
                "MLEADER  Specify arrowhead point:".into(),
            Step::CollectPoints { verts } =>
                format!("MLEADER  Specify next point [{} pts — Enter to finish]:", verts.len()),
            Step::AskText { verts } =>
                format!("MLEADER  Enter annotation text [{} pts — blank = no text]:", verts.len()),
        }
    }

    fn wants_text_input(&self) -> bool {
        matches!(self.step, Step::AskText { .. })
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Step::CollectPoints { verts } = &mut self.step {
            verts.push(pt);
        }
        CmdResult::NeedPoint
    }

    fn on_enter(&mut self) -> CmdResult {
        if let Step::CollectPoints { verts } = &self.step {
            if verts.len() < 2 { return CmdResult::Cancel; }
            let verts = verts.clone();
            self.step = Step::AskText { verts };
            CmdResult::NeedPoint
        } else {
            CmdResult::Cancel
        }
    }

    fn on_text_input(&mut self, raw: &str) -> Option<CmdResult> {
        if let Step::AskText { verts } = &self.step {
            let text = raw.trim();
            let ml = build_mleader(text, verts);
            Some(CmdResult::CommitAndExit(EntityType::MultiLeader(ml)))
        } else {
            None
        }
    }

    fn on_escape(&mut self) -> CmdResult { CmdResult::Cancel }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if let Step::CollectPoints { verts } = &self.step {
            if verts.is_empty() { return None; }
            let mut pts = verts.clone();
            pts.push(pt);
            Some(preview_wire(&pts))
        } else {
            None
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn v3(p: Vec3) -> Vector3 { Vector3::new(p.x as f64, p.y as f64, p.z as f64) }

fn build_mleader(text: &str, verts: &[Vec3]) -> MultiLeader {
    // Last vertex = content/text location; remaining = leader line points
    let (leader_pts, content_pt) = verts.split_at(verts.len() - 1);
    let content_pt = content_pt[0];

    let leader_v3: Vec<Vector3> = leader_pts.iter().map(|p| v3(*p)).collect();
    let content_v3 = v3(content_pt);

    let mut ml = MultiLeader::with_text(text, content_v3, leader_v3);

    // Match Leader entity defaults
    ml.text_height          = 2.5;
    ml.context.text_height  = 2.5;
    ml.arrowhead_size       = 2.5;
    ml.dogleg_length        = 2.5;

    // Direction: from last leader pt toward content
    if let (Some(last_leader), Some(root)) = (leader_pts.last(), ml.context.leader_roots.first_mut()) {
        let dx = (content_pt.x - last_leader.x) as f64;
        let dy = (content_pt.y - last_leader.y) as f64;
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        root.direction = Vector3::new(dx / len, dy / len, 0.0);
        root.connection_point = content_v3;
        root.landing_distance = 2.5;
    }

    ml
}

fn preview_wire(pts: &[Vec3]) -> WireModel {
    let mut points: Vec<[f32; 3]> = pts.iter().map(|p| [p.x, p.y, p.z]).collect();
    if pts.len() >= 2 {
        let [w1, w2] = arrowhead_wings(pts[0], pts[1], 2.0);
        points.push([f32::NAN; 3]);
        points.push([w1.x, w1.y, w1.z]);
        points.push([pts[0].x, pts[0].y, pts[0].z]);
        points.push([w2.x, w2.y, w2.z]);
    }
    WireModel {
        name: "mleader_preview".into(),
        points,
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
    }
}

fn arrowhead_wings(tip: Vec3, next: Vec3, size: f32) -> [Vec3; 2] {
    let d = next - tip;
    let len = (d.x * d.x + d.y * d.y).sqrt().max(1e-9);
    let (dx, dy) = (d.x / len, d.y / len);
    let angle = std::f32::consts::PI / 6.0;
    let (s, c) = angle.sin_cos();
    [
        Vec3::new(tip.x + (dx*c - dy*s)*size, tip.y + (dx*s + dy*c)*size, tip.z),
        Vec3::new(tip.x + (dx*c + dy*s)*size, tip.y + (-dx*s + dy*c)*size, tip.z),
    ]
}
