// MLINE command — create a multiline (parallel lines).
//
// Workflow: pick vertices, Enter to finish.
// Text input (when >= 1 point picked):
//   C / CLOSE  → close and commit
//   S <value>  → set scale factor then continue picking

use acadrust::entities::MLine;
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;

pub struct MlineCommand {
    points: Vec<Vec3>,
    scale: f64,
    waiting_scale: bool,
    style_name: String,
}

impl MlineCommand {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self { points: vec![], scale: 1.0, waiting_scale: false, style_name: "Standard".into() }
    }

    pub fn with_style(style_name: impl Into<String>) -> Self {
        Self { points: vec![], scale: 1.0, waiting_scale: false, style_name: style_name.into() }
    }
}

impl CadCommand for MlineCommand {
    fn name(&self) -> &'static str { "MLINE" }

    fn prompt(&self) -> String {
        if self.waiting_scale {
            "MLINE  Enter scale factor:".into()
        } else if self.points.is_empty() {
            format!("MLINE  Specify start point (scale={:.2}):", self.scale)
        } else {
            format!("MLINE  Specify next point ({} pts, Enter to finish, C to close, S to set scale):", self.points.len())
        }
    }

    fn wants_text_input(&self) -> bool {
        self.waiting_scale || !self.points.is_empty()
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        // Waiting for scale value
        if self.waiting_scale {
            let v: f64 = text.trim().replace(',', ".").parse().ok().filter(|&v: &f64| v > 0.0)?;
            self.scale = v;
            self.waiting_scale = false;
            return Some(CmdResult::NeedPoint);
        }

        let up = text.trim().to_uppercase();

        // Close command
        if (up == "C" || up == "CLOSE") && self.points.len() >= 3 {
            let entity = build_mline(&self.points, self.scale, true, &self.style_name);
            return Some(CmdResult::CommitAndExit(entity));
        }

        // Scale: "S" alone → prompt for value
        if up == "S" {
            self.waiting_scale = true;
            return Some(CmdResult::NeedPoint);
        }

        // Scale: "S <value>" inline
        if let Some(rest) = up.strip_prefix("S ") {
            if let Ok(v) = rest.trim().replace(',', ".").parse::<f64>() {
                if v > 0.0 { self.scale = v; }
                return Some(CmdResult::NeedPoint);
            }
        }

        None
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        self.points.push(pt);
        CmdResult::NeedPoint
    }

    fn on_enter(&mut self) -> CmdResult {
        if self.points.len() < 2 {
            return CmdResult::Cancel;
        }
        let entity = build_mline(&self.points, self.scale, false, &self.style_name);
        CmdResult::CommitAndExit(entity)
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if self.points.is_empty() { return None; }
        let mut pts: Vec<[f32; 3]> =
            self.points.iter().map(|p| [p.x, p.y, p.z]).collect();
        pts.push([pt.x, pt.y, pt.z]);
        Some(WireModel {
            name: "mline_preview".into(),
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

fn build_mline(pts: &[Vec3], scale: f64, closed: bool, style_name: &str) -> EntityType {
    let verts: Vec<Vector3> = pts.iter()
        .map(|p| Vector3::new(p.x as f64, p.z as f64, p.y as f64))
        .collect();
    let mut mline = if closed {
        MLine::closed_from_points(&verts)
    } else {
        MLine::from_points(&verts)
    };
    mline.scale_factor = scale;
    mline.style_name = style_name.to_string();
    EntityType::MLine(mline)
}
