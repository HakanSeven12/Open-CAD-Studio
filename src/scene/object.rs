// Shared value types used by the dispatch and grip systems.

use acadrust::types::{Color as AcadColor, LineWeight};
use glam::Vec3;

/// The kind of value held by a property row.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    /// Read-only display text.
    ReadOnly(String),
    /// Editable numeric/text field.
    EditText(String),
    /// Layer name — rendered as a combo_box.
    LayerChoice(String),
    /// Generic string choice rendered as a combo_box.
    Choice {
        selected: String,
        options: Vec<String>,
    },
    /// ACI/RGB/ByLayer/ByBlock color — rendered as a color picker.
    ColorChoice(AcadColor),
    /// Color varies across the current multi-selection.
    ColorVaries,
    /// Line weight — rendered as a combo_box.
    LwChoice(LineWeight),
    /// Lineweight varies across the current multi-selection.
    LwVaries,
    /// Linetype name — rendered as a combo_box.
    LinetypeChoice(String),
    /// Boolean flag — rendered as a toggle button (e.g. Invisible).
    BoolToggle { field: &'static str, value: bool },
    /// Hatch pattern name — rendered as a combo_box from the catalog.
    HatchPatternChoice(String),
}

/// A single property row in the Properties panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub label: String,
    /// Stable field identifier used in `PropGeomInput` / `PropGeomCommit` messages.
    pub field: &'static str,
    pub value: PropValue,
}

/// A named section of properties (e.g. "General", "Geometry").
#[derive(Clone, Debug, PartialEq)]
pub struct PropSection {
    pub title: String,
    pub props: Vec<Property>,
}

// ── Grip types ─────────────────────────────────────────────────────────────

/// Visual marker shape for a grip point. The complete vocabulary that
/// matches the standard CAD grip conventions:
/// * `Square` — endpoint / vertex / centre. Drag → moves a single
///   point or translates the entity.
/// * `Rectangle` — direction-aware mid-segment stretch handle.
///   Drawn as a small box rotated along `dir` (the in-plane segment
///   direction in world XY). Used for polyline / wipeout / image /
///   dimension segment midpoints.
/// * `Triangle` — directional indicator (Phase 2: dynamic-block
///   parameters, dimension reverse-arrow flips, multi-functional
///   hover popups).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GripShape {
    Square,
    Rectangle,
    Triangle,
}

/// Describes one grip point for an entity.
#[derive(Clone, Debug)]
pub struct GripDef {
    /// Object-local identifier (stable index, unique per object instance).
    pub id: usize,
    /// World-space position of the grip.
    pub world: Vec3,
    /// `true` → midpoint / centre grip (drags the whole shape).
    /// `false` → endpoint / vertex grip (moves a single point).
    pub is_midpoint: bool,
    /// Visual marker shape for the grip.
    pub shape: GripShape,
    /// World-XY direction vector used to orient a `Rectangle` grip
    /// along its segment. `None` for shapes that don't need rotation
    /// (Square, Triangle in non-directional contexts).
    pub dir: Option<[f32; 2]>,
}

/// How to apply a grip drag result.
#[derive(Clone, Debug)]
pub enum GripApply {
    /// Move a specific vertex to this absolute world position.
    Absolute(Vec3),
    /// Translate the whole object by this delta vector.
    Translate(Vec3),
}

/// One entry in the hover-popup menu that opens when the cursor dwells
/// on a grip. The `label` is the user-visible string; `action` is the
/// operation the entity will perform when the item is committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GripMenuItem {
    pub label: &'static str,
    pub action: GripMenuAction,
}

/// All operations a grip popup menu can dispatch. Entity-specific code
/// in `apply_grip_menu` decodes these into edits. `Stretch` is the
/// default no-op-equivalent — picking it just starts the regular
/// stretch drag, identical to clicking the grip with no popup open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GripMenuAction {
    Stretch,
    Lengthen,
    Radius,
    ArcLength,
    AddVertex,
    RemoveVertex,
    ConvertToArc,
    ConvertToLine,
    StretchVertex,
    AddLeader,
    RemoveLeader,
    ReverseArrows,
    MoveWithDimLine,
    MoveWithLeader,
    MoveIndependent,
    ResetText,
    RotateText,
    AboveDimLine,
    Center,
    OriginPoint,
    HatchAngle,
    HatchScale,
    HatchPattern,
    TangentDirection,
}
