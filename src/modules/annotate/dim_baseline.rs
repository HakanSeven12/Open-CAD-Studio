// DIMBASELINE command — stacked baseline dimensions all measured from the same origin.
//
// Each new point becomes the second extension line origin of a new dimension.
// The first extension line is always the same base origin point.
// Each new dimension line is placed further from the baseline by DIMDLI (increment).
//
// Constructed from commands.rs after finding the last placed linear/aligned dimension.

use acadrust::entities::{Dimension, DimensionLinear};
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/dim_baseline.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "DIMBASELINE",
        label: "Baseline",
        icon: ICON,
        event: ModuleEvent::Command("DIMBASELINE".to_string()),
    }
}

/// Default stacking increment (world units) between successive baseline dimensions.
const DIMDLI: f32 = 1.5;

pub struct DimBaselineCommand {
    /// Fixed first-extension-line origin (never changes).
    base_p1: Vec3,
    /// Direction along the measurement direction (0.0 = horizontal, PI/2 = vertical).
    rotation: f64,
    /// Unit vector perpendicular to the dimension axis, pointing toward the dim line side.
    perp: Vec3,
    /// Perpendicular distance of the NEXT dimension line from the extension-line axis.
    next_offset: f32,
    /// True once we have a base dimension loaded.
    ready: bool,
}

impl DimBaselineCommand {
    /// No base dim found — cancel immediately.
    pub fn new() -> Self {
        Self {
            base_p1: Vec3::ZERO,
            rotation: 0.0,
            perp: Vec3::Y,
            next_offset: DIMDLI,
            ready: false,
        }
    }

    /// Build from the last placed dimension.
    ///
    /// `p1` — first extension line origin (fixed baseline).
    /// `p2` — second extension line origin of the base dim (unused for placement, kept for context).
    /// `definition_point` — dim-line position of the base dim (defines perpendicular side).
    /// `rotation` — 0.0 = horizontal, PI/2 = vertical.
    pub fn from_base(p1: Vec3, _p2: Vec3, definition_point: Vec3, rotation: f64) -> Self {
        let axis = if rotation.abs() < 0.1 { Vec3::X } else { Vec3::Y };
        let perp = Vec3::new(-axis.y, axis.x, 0.0);
        let base_offset = (definition_point - p1).dot(perp);
        // Next baseline dim goes one DIMDLI further from the baseline.
        let next_offset = base_offset + DIMDLI;
        Self {
            base_p1: p1,
            rotation,
            perp,
            next_offset,
            ready: true,
        }
    }
}

impl CadCommand for DimBaselineCommand {
    fn name(&self) -> &'static str { "DIMBASELINE" }

    fn prompt(&self) -> String {
        if !self.ready {
            "DIMBASELINE  No base dimension found. Place a dimension first.".into()
        } else {
            "DIMBASELINE  Specify a second extension line origin (Enter to exit):".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if !self.ready {
            return CmdResult::Cancel;
        }
        let p1 = self.base_p1;
        let p2 = pt;

        // Build a new linear dimension.
        let mut dim = DimensionLinear::new(v3(p1), v3(p2));
        dim.rotation = self.rotation;

        let dim_line_pt = p1 + self.perp * self.next_offset;
        let dim_line_pt2 = p2 + self.perp * self.next_offset;
        dim.definition_point = v3(dim_line_pt);
        dim.base.definition_point = v3(dim_line_pt);
        dim.base.text_middle_point = v3((dim_line_pt + dim_line_pt2) * 0.5);
        dim.base.insertion_point = dim.base.text_middle_point;
        dim.base.actual_measurement = dim.measurement();

        // Stack the next dim line further out.
        self.next_offset += DIMDLI;

        CmdResult::CommitEntity(EntityType::Dimension(Dimension::Linear(dim)))
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if !self.ready {
            return None;
        }
        let p1 = self.base_p1;
        let dim_line_pt = p1 + self.perp * self.next_offset;
        let dim_line_pt2 = pt + self.perp * self.next_offset;
        Some(WireModel {
            name: "dimbase_preview".into(),
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
