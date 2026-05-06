// REVCLOUD command — draw a revision cloud (arc-bumped closed polyline).
//
// Workflow: pick polygon corners (like PLINE), press Enter to close.
// Each segment of the polygon is subdivided into arc bumps (bulge = 0.5).
// Minimum arc length = `arc_length` parameter.

use acadrust::entities::LwPolyline;
use acadrust::types::Vector2;
use acadrust::{EntityType, entities::LwVertex};
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../../assets/icons/revcloud.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "REVCLOUD",
        label: "Rev Cloud",
        icon: ICON,
        event: ModuleEvent::Command("REVCLOUD".to_string()),
    }
}

const DEFAULT_ARC_LEN: f64 = 1.0; // default arc bump length

pub struct RevCloudCommand {
    points: Vec<Vec3>,
    arc_length: f64,
}

impl RevCloudCommand {
    pub fn new() -> Self {
        Self {
            points: vec![],
            arc_length: DEFAULT_ARC_LEN,
        }
    }
}

impl CadCommand for RevCloudCommand {
    fn name(&self) -> &'static str { "REVCLOUD" }

    fn prompt(&self) -> String {
        if self.points.is_empty() {
            format!("REVCLOUD  Specify start point (arc length = {:.2}):", self.arc_length)
        } else {
            format!("REVCLOUD  Specify next point ({} pts, Enter to close):", self.points.len())
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
        let entity = make_revcloud(&self.points, self.arc_length);
        CmdResult::CommitAndExit(entity)
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if self.points.is_empty() { return None; }
        let mut preview_pts: Vec<[f32; 3]> =
            self.points.iter().map(|p| [p.x, p.y, p.z]).collect();
        preview_pts.push([pt.x, pt.y, pt.z]);
        preview_pts.push([self.points[0].x, self.points[0].y, self.points[0].z]);
        Some(WireModel {
            name: "revcloud_preview".into(),
            points: preview_pts,
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

fn make_revcloud(pts: &[Vec3], arc_len: f64) -> EntityType {
    let n = pts.len();
    let mut vertices: Vec<LwVertex> = Vec::new();

    // For each edge, subdivide into arc bumps (bulge ≈ 0.5)
    let bump_bulge = 0.5f64; // tan(included_angle/4) ≈ 0.5 → ~53° arc

    for i in 0..n {
        let p0 = pts[i];
        let p1 = pts[(i + 1) % n];
        let seg_len = ((p1.x - p0.x).powi(2) + (p1.z - p0.z).powi(2)).sqrt() as f64;
        if seg_len < 1e-6 { continue; }

        // How many full arcs fit?
        let num_arcs = ((seg_len / arc_len).round() as usize).max(1);
        let step_x = (p1.x - p0.x) as f64 / num_arcs as f64;
        let step_z = (p1.z - p0.z) as f64 / num_arcs as f64;

        for j in 0..num_arcs {
            let x = p0.x as f64 + step_x * j as f64;
            let y = p0.z as f64 + step_z * j as f64; // DXF Y
            let mut v = LwVertex::new(Vector2::new(x, y));
            v.bulge = bump_bulge;
            vertices.push(v);
        }
    }

    let mut p = LwPolyline::new();
    p.is_closed = true;
    p.vertices = vertices;
    EntityType::LwPolyline(p)
}
