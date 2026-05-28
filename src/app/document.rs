use crate::command::CadCommand;
use crate::linetypes;
use crate::modules::home::modify::refedit::RefEditSession;
use crate::scene::grip::GripEdit;
use crate::scene::GripDef;
use crate::scene::Scene;
use crate::snap::SnapResult;
use crate::ui::{LayerPanel, PropertiesPanel};
use acadrust::tables::Ucs;
use acadrust::{CadDocument, Handle};
use iced;
use std::path::PathBuf;

// ── Dynamic input ──────────────────────────────────────────────────────────

/// One quantity shown in the dynamic-input overlay near the cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DynComponent {
    /// Absolute X ordinate.
    X,
    /// Absolute Y ordinate.
    Y,
    /// Absolute Z ordinate (only visible after the user types a second
    /// `,` separator from a cartesian X/Y configuration).
    Z,
    /// Linear distance from the last point.
    Distance,
    /// Angle from the last point, in degrees.
    Angle,
}

/// A single editable dynamic-input field. `buffer == None` means the box
/// tracks the cursor live; once the user types, the typed text is held in
/// `buffer` and the box stops following the cursor (it is "locked").
#[derive(Clone, Debug)]
pub(super) struct DynFieldEntry {
    pub(super) component: DynComponent,
    pub(super) buffer: Option<String>,
}

impl DynFieldEntry {
    pub(super) fn new(component: DynComponent) -> Self {
        Self {
            component,
            buffer: None,
        }
    }
    pub(super) fn locked(&self) -> bool {
        self.buffer.is_some()
    }
}

// ── Per-document tab state ─────────────────────────────────────────────────

pub(super) struct DocumentTab {
    pub(super) scene: Scene,
    pub(super) current_path: Option<PathBuf>,
    pub(super) dirty: bool,
    pub(super) tab_title: String,
    pub(super) properties: PropertiesPanel,
    pub(super) layers: LayerPanel,
    pub(super) active_cmd: Option<Box<dyn CadCommand>>,
    pub(super) last_cmd: Option<String>,
    pub(super) snap_result: Option<SnapResult>,
    pub(super) active_grip: Option<GripEdit>,
    pub(super) selected_grips: Vec<GripDef>,
    pub(super) selected_handle: Option<Handle>,
    pub(super) wireframe: bool,
    pub(super) render_mode: acadrust::entities::ViewportRenderMode,
    pub(super) visual_style: String,
    pub(super) last_cursor_world: glam::Vec3,
    pub(super) last_cursor_screen: iced::Point,
    /// Dynamic-input fields shown near the cursor while a command waits
    /// for a point/distance/angle. Rebuilt whenever the active command's
    /// `dyn_field()` or the presence of a base point changes. Empty when
    /// dynamic input is not active.
    pub(super) dyn_fields: Vec<DynFieldEntry>,
    /// Index of the field that TAB has focused (the one keystrokes edit).
    pub(super) dyn_active: usize,
    pub(super) history: HistoryState,
    pub(super) active_layer: String,
    /// Currently active UCS. `None` means WCS (identity transform).
    pub(super) active_ucs: Option<Ucs>,
    /// Custom model-space background color.  `None` = default dark grey.
    pub(super) bg_color: Option<[f32; 4]>,
    /// Custom paper-space background color.  `None` = default off-white grey.
    pub(super) paper_bg_color: Option<[f32; 4]>,
    /// Active REFEDIT session, if any.
    pub(super) refedit_session: Option<RefEditSession>,
    /// Currently active MLeader style name.
    pub(super) active_mleader_style: String,
    /// Last camera_generation value written back to the document.
    pub(super) last_synced_camera_gen: u64,
    /// Sentinel "Welcome / Start" tab. Always at index 0 when present.
    /// Cannot be closed; the viewport area renders a welcome page instead
    /// of the model-space shader. The scene is still constructed so the
    /// rest of the code can treat it as a normal tab when reading.
    pub(super) is_start: bool,
}

impl DocumentTab {
    pub(super) fn new_drawing(n: usize) -> Self {
        let mut scene = Scene::new();
        linetypes::populate_document(&mut scene.document);
        // Override acadrust's imperial default limits (12×9) with A4 landscape.
        for obj in scene.document.objects.values_mut() {
            if let acadrust::objects::ObjectType::Layout(l) = obj {
                if l.name != "Model" {
                    l.min_limits = (0.0, 0.0);
                    l.max_limits = (297.0, 210.0);
                    l.min_extents = (0.0, 0.0, 0.0);
                    l.max_extents = (297.0, 210.0, 0.0);
                }
            }
        }
        Self {
            scene,
            current_path: None,
            dirty: false,
            tab_title: format!("Drawing{}", n),
            properties: PropertiesPanel::empty(),
            layers: LayerPanel::default(),
            active_cmd: None,
            last_cmd: None,
            snap_result: None,
            active_grip: None,
            selected_grips: vec![],
            selected_handle: None,
            wireframe: false,
            render_mode: acadrust::entities::ViewportRenderMode::Wireframe2D,
            visual_style: "Wireframe 2D".into(),
            last_cursor_world: glam::Vec3::ZERO,
            last_cursor_screen: iced::Point::ORIGIN,
            dyn_fields: Vec::new(),
            dyn_active: 0,
            history: HistoryState::default(),
            active_layer: "0".to_string(),
            active_ucs: None,
            bg_color: None,
            paper_bg_color: None,
            refedit_session: None,
            active_mleader_style: "Standard".to_string(),
            last_synced_camera_gen: 0,
            is_start: false,
        }
    }

    /// Welcome / Start tab. Carries a dummy Scene so the rest of the app
    /// can read tab state uniformly; the viewport renderer detects
    /// `is_start` and shows a welcome page instead.
    pub(super) fn new_start() -> Self {
        let mut t = Self::new_drawing(0);
        t.tab_title = "Start".to_string();
        t.is_start = true;
        t
    }

    pub(super) fn tab_display_name(&self) -> String {
        match &self.current_path {
            Some(p) => p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            None => self.tab_title.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct HistorySnapshot {
    pub(super) document: CadDocument,
    pub(super) current_layout: String,
    pub(super) selected: Vec<Handle>,
    pub(super) dirty: bool,
    pub(super) label: String,
}

#[derive(Default)]
pub(super) struct HistoryState {
    pub(super) undo_stack: Vec<HistorySnapshot>,
    pub(super) redo_stack: Vec<HistorySnapshot>,
}
