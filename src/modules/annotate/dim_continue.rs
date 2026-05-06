// DIMCONTINUE command — chain linear/aligned dimensions end-to-end.
//
// Each new point becomes the second extension line origin of a new dimension,
// whose first extension line origin is the second extension line of the previous dim.
// The dimension line stays at the same perpendicular offset as the base dimension.
//
// Constructed from commands.rs after finding the last placed linear/aligned dimension.

use acadrust::entities::{Dimension, DimensionLinear};
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_continue.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMCONTINUE",
        label: "Continue",
        icon: ICON,
        event: ModuleEvent::Command("DIMCONTINUE".to_string()),
    }
}

pub struct DimContinueCommand {
    /// Fixed first-extension-line origin for the current step (moves each iteration).
    chain_p1: Vec3,
    /// Direction along the dimension axis (0.0 = horizontal, PI/2 = vertical).
    rotation: f64,
    /// Perpendicular distance from the extension-line axis to the dimension line.
    /// Preserved from the base dimension.
    dim_offset: f32,
    /// Direction of "up" perpendicular to the dim axis (points toward the dim line).
    perp: Vec3,
    /// True once we have a base dimension loaded.
    ready: bool,
}

impl DimContinueCommand {
    /// No base dim found — will show an error prompt and cancel immediately.
    pub fn new() -> Self {
        Self {
            chain_p1: Vec3::ZERO,
            rotation: 0.0,
            dim_offset: 0.0,
            perp: Vec3::Y,
            ready: false,
        }
    }

    /// Build from the last placed dimension.
    ///
    /// `p1` / `p2` — extension line origins of the base dim.
    /// `definition_point` — where the dim line was placed (defines perpendicular offset).
    /// `rotation` — 0.0 = horizontal dim, PI/2 = vertical dim.
    pub fn from_base(p1: Vec3, p2: Vec3, definition_point: Vec3, rotation: f64) -> Self {
        // Axis unit vector along the measurement direction.
        let axis = if rotation.abs() < 0.1 { Vec3::X } else { Vec3::Y };
        // Perpendicular unit vector toward the dim line.
        let perp = Vec3::new(-axis.y, axis.x, 0.0);
        let dim_offset = (definition_point - p1).dot(perp);
        Self {
            chain_p1: p2,
            rotation,
            dim_offset,
            perp,
            ready: true,
        }
    }
}

impl CadCommand for DimContinueCommand {
    fn name(&self) -> &'static str { "DIMCONTINUE" }

    fn prompt(&self) -> String {
        if !self.ready {
            "DIMCONTINUE  No base dimension found. Place a dimension first.".into()
        } else {
            "DIMCONTINUE  Specify a second extension line origin (Enter to exit):".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if !self.ready {
            return CmdResult::Cancel;
        }
        let p1 = self.chain_p1;
        let p2 = pt;

        // Build a new linear dimension.
        let mut dim = DimensionLinear::new(v3(p1), v3(p2));
        dim.rotation = self.rotation;

        // Dim-line position: same perpendicular distance as base.
        let dim_line_pt = p1 + self.perp * self.dim_offset;
        dim.definition_point = v3(dim_line_pt);
        dim.base.definition_point = v3(dim_line_pt);
        dim.base.text_middle_point = v3((dim_line_pt + (p2 + self.perp * self.dim_offset)) * 0.5);
        dim.base.insertion_point = dim.base.text_middle_point;
        dim.base.actual_measurement = dim.measurement();

        // Advance chain: next dim's P1 = this dim's P2.
        self.chain_p1 = p2;

        CmdResult::CommitEntity(EntityType::Dimension(Dimension::Linear(dim)))
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if !self.ready {
            return None;
        }
        let p1 = self.chain_p1;
        let dim_line_pt = p1 + self.perp * self.dim_offset;
        let dim_line_pt2 = pt + self.perp * self.dim_offset;
        Some(WireModel {
            name: "dimcont_preview".into(),
            points: vec![
                [p1.x, p1.y, p1.z], [dim_line_pt.x, dim_line_pt.y, dim_line_pt.z],
                [f32::NAN, 0.0, 0.0],
                [pt.x, pt.y, pt.z], [dim_line_pt2.x, dim_line_pt2.y, dim_line_pt2.z],
                [f32::NAN, 0.0, 0.0],
                [dim_line_pt.x, dim_line_pt.y, dim_line_pt.z],
                [dim_line_pt2.x, dim_line_pt2.y, dim_line_pt2.z],
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

fn v3(p: Vec3) -> Vector3 {
    Vector3::new(p.x as f64, p.y as f64, p.z as f64)
}
