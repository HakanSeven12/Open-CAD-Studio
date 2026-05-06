// WIPEOUT command — create a rectangular or polygonal masking area.
//
// Modes:
//   WIPEOUT (default): two-corner rectangular wipeout
//   WIPEOUT P:         polygonal wipeout (pick corners, Enter to close)

use acadrust::entities::Wipeout;
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../../assets/icons/wipeout.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "WIPEOUT",
        label: "Wipeout",
        icon: ICON,
        event: ModuleEvent::Command("WIPEOUT".to_string()),
    }
}

pub struct WipeoutCommand {
    mode: WipeoutMode,
    first: Option<Vec3>,
    poly_pts: Vec<Vec3>,
}

enum WipeoutMode {
    Rectangular,
    Polygonal,
}

impl WipeoutCommand {
    pub fn new_rectangular() -> Self {
        Self { mode: WipeoutMode::Rectangular, first: None, poly_pts: vec![] }
    }
    pub fn new_polygonal() -> Self {
        Self { mode: WipeoutMode::Polygonal, first: None, poly_pts: vec![] }
    }
}

impl CadCommand for WipeoutCommand {
    fn name(&self) -> &'static str { "WIPEOUT" }

    fn prompt(&self) -> String {
        match &self.mode {
            WipeoutMode::Rectangular => {
                if self.first.is_none() {
                    "WIPEOUT  Specify first corner:".into()
                } else {
                    "WIPEOUT  Specify opposite corner:".into()
                }
            }
            WipeoutMode::Polygonal => {
                if self.poly_pts.is_empty() {
                    "WIPEOUT (Polygon)  Specify first point:".into()
                } else {
                    format!("WIPEOUT (Polygon)  Specify next point ({} pts, Enter to close):", self.poly_pts.len())
                }
            }
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match &self.mode {
            WipeoutMode::Rectangular => {
                if let Some(p1) = self.first {
                    let entity = make_rect_wipeout(p1, pt);
                    CmdResult::CommitAndExit(entity)
                } else {
                    self.first = Some(pt);
                    CmdResult::NeedPoint
                }
            }
            WipeoutMode::Polygonal => {
                self.poly_pts.push(pt);
                CmdResult::NeedPoint
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        match &self.mode {
            WipeoutMode::Polygonal if self.poly_pts.len() >= 3 => {
                let entity = make_poly_wipeout(&self.poly_pts);
                CmdResult::CommitAndExit(entity)
            }
            _ => CmdResult::Cancel,
        }
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        match &self.mode {
            WipeoutMode::Rectangular => {
                let p1 = self.first?;
                let min = p1.min(pt);
                let max = p1.max(pt);
                Some(WireModel {
                    name: "wipeout_preview".into(),
                    points: vec![
                        [min.x, min.y, min.z],
                        [max.x, min.y, min.z],
                        [max.x, max.y, max.z],
                        [min.x, max.y, max.z],
                        [min.x, min.y, min.z],
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
            WipeoutMode::Polygonal => {
                if self.poly_pts.is_empty() { return None; }
                let mut pts: Vec<[f32; 3]> =
                    self.poly_pts.iter().map(|p| [p.x, p.y, p.z]).collect();
                pts.push([pt.x, pt.y, pt.z]);
                pts.push([self.poly_pts[0].x, self.poly_pts[0].y, self.poly_pts[0].z]);
                Some(WireModel {
                    name: "wipeout_poly_preview".into(),
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
    }
}

fn make_rect_wipeout(p1: Vec3, p2: Vec3) -> EntityType {
    // Y-up world: X→DXF X, Z→DXF Y, Y→DXF Z
    let c1 = Vector3::new(
        p1.x.min(p2.x) as f64,
        p1.z.min(p2.z) as f64,
        p1.y as f64,
    );
    let c2 = Vector3::new(
        p1.x.max(p2.x) as f64,
        p1.z.max(p2.z) as f64,
        p1.y as f64,
    );
    EntityType::Wipeout(Wipeout::from_corners(c1, c2))
}

fn make_poly_wipeout(pts: &[Vec3]) -> EntityType {
    use acadrust::types::Vector2;
    let z = pts[0].y as f64;
    let verts: Vec<Vector2> = pts.iter()
        .map(|p| Vector2::new(p.x as f64, p.z as f64))
        .collect();
    EntityType::Wipeout(Wipeout::polygonal(&verts, z))
}
