// LEADER command
//
// Flow:
//   1. CollectPoints  — click arrowhead, then bend points; Enter (≥2) to finish
//   2. AskCreationType— wants_text_input; N/T/B/TL → default Text on blank Enter
//   3. AskAnnotation  — text string (Text) or block name (Block); blank = skip
//   → commit Leader  [+ MText | + Insert]

use acadrust::entities::{Insert, Leader, LeaderCreationType, MText};
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/leader.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "LEADER",
        label: "Leader",
        icon: ICON,
        event: ModuleEvent::Command("LEADER".to_string()),
    }
}

enum Step {
    CollectPoints { verts: Vec<Vec3> },
    AskCreationType { verts: Vec<Vec3> },
    AskText { verts: Vec<Vec3> },
    AskBlock { verts: Vec<Vec3> },
}

pub struct LeaderCommand {
    step: Step,
}

impl LeaderCommand {
    pub fn new() -> Self {
        Self { step: Step::CollectPoints { verts: Vec::new() } }
    }
}

impl CadCommand for LeaderCommand {
    fn name(&self) -> &'static str { "LEADER" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::CollectPoints { verts } if verts.is_empty() =>
                "LEADER  Specify arrowhead point:".into(),
            Step::CollectPoints { verts } =>
                format!("LEADER  Specify next point [{} pts — Enter to finish]:", verts.len()),
            Step::AskCreationType { .. } =>
                "LEADER  Annotation type [None/Text/Block/Tolerance] <Text>:".into(),
            Step::AskText { verts } =>
                format!("LEADER  Annotation text [{} pts — blank = skip]:", verts.len()),
            Step::AskBlock { verts } =>
                format!("LEADER  Block name [{} pts — blank = skip]:", verts.len()),
        }
    }

    fn wants_text_input(&self) -> bool {
        !matches!(self.step, Step::CollectPoints { .. })
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
            self.step = Step::AskCreationType { verts };
            CmdResult::NeedPoint
        } else {
            CmdResult::Cancel
        }
    }

    fn on_text_input(&mut self, raw: &str) -> Option<CmdResult> {
        let text = raw.trim();
        match &self.step {
            Step::AskCreationType { verts } => {
                let verts = verts.clone();
                match parse_ct(text) {
                    LeaderCreationType::NoAnnotation | LeaderCreationType::WithTolerance => {
                        let ct = parse_ct(text);
                        Some(CmdResult::CommitAndExit(EntityType::Leader(build_leader(&verts, ct))))
                    }
                    LeaderCreationType::WithBlock => {
                        self.step = Step::AskBlock { verts };
                        Some(CmdResult::NeedPoint)
                    }
                    LeaderCreationType::WithText => {
                        self.step = Step::AskText { verts };
                        Some(CmdResult::NeedPoint)
                    }
                }
            }
            Step::AskText { verts } => {
                let verts = verts.clone();
                let leader = build_leader(&verts, LeaderCreationType::WithText);
                if text.is_empty() {
                    return Some(CmdResult::CommitAndExit(EntityType::Leader(leader)));
                }
                let mtext = build_mtext(text, landing_pt(&verts, leader.text_height), leader.text_height);
                Some(CmdResult::ReplaceMany(
                    vec![],
                    vec![EntityType::Leader(leader), EntityType::MText(mtext)],
                ))
            }
            Step::AskBlock { verts } => {
                let verts = verts.clone();
                let leader = build_leader(&verts, LeaderCreationType::WithBlock);
                if text.is_empty() {
                    return Some(CmdResult::CommitAndExit(EntityType::Leader(leader)));
                }
                let ins = Insert::new(text, v3(landing_pt(&verts, leader.text_height)));
                Some(CmdResult::ReplaceMany(
                    vec![],
                    vec![EntityType::Leader(leader), EntityType::Insert(ins)],
                ))
            }
            Step::CollectPoints { .. } => None,
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

fn parse_ct(s: &str) -> LeaderCreationType {
    match s.to_ascii_uppercase().as_str() {
        "N" | "NONE"      => LeaderCreationType::NoAnnotation,
        "B" | "BLOCK"     => LeaderCreationType::WithBlock,
        "TL"| "TOLERANCE" => LeaderCreationType::WithTolerance,
        _                 => LeaderCreationType::WithText,
    }
}

fn v3(p: Vec3) -> Vector3 { Vector3::new(p.x as f64, p.y as f64, p.z as f64) }

fn build_leader(verts: &[Vec3], ct: LeaderCreationType) -> Leader {
    let mut l = Leader::from_vertices(verts.iter().map(|p| v3(*p)).collect());
    l.creation_type = ct;
    l.hookline_enabled = !matches!(ct, LeaderCreationType::NoAnnotation);
    l
}

fn landing_pt(verts: &[Vec3], text_height: f64) -> Vec3 {
    let last = *verts.last().unwrap();
    let prev = verts[verts.len() - 2];
    let sign = if last.x >= prev.x { 1.0_f32 } else { -1.0_f32 };
    Vec3::new(last.x + sign * text_height as f32 * 1.5, last.y, last.z)
}

fn build_mtext(text: &str, pos: Vec3, height: f64) -> MText {
    let mut m = MText::new();
    m.value = text.to_string();
    m.insertion_point = v3(pos);
    m.height = height;
    m
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
        name: "leader_preview".into(),
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
            fill_tris: vec![],
    }
}

pub fn arrowhead_wings(tip: Vec3, next: Vec3, size: f32) -> [Vec3; 2] {
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
