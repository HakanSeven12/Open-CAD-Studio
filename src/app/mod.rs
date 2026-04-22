mod document;
mod helpers;
mod history;
mod properties;
mod layers;
mod commands;
mod cmd_result;
mod view;
mod update;

use document::DocumentTab;

use acadrust::types::{Color as AcadColor, LineWeight};
use acadrust::CadDocument;
use crate::modules::ModuleEvent;
use crate::scene::CubeRegion;
use crate::snap::Snapper;
use crate::ui::{AppMenu, CommandLine, Ribbon, StatusBar};

use iced::time::Instant;
use iced::window;
use iced::{mouse, Point, Task, Theme};

pub(super) const POLY_START_DELAY_MS: u128 = 150;
pub(super) const VARIES_LABEL: &str = "*VARIES*";

// ── Application state ──────────────────────────────────────────────────────

pub(super) struct H7CAD {
    start: Instant,
    tabs: Vec<DocumentTab>,
    active_tab: usize,
    tab_counter: usize,
    ribbon: Ribbon,
    app_menu: AppMenu,
    command_line: CommandLine,
    status_bar: StatusBar,
    cursor_pos: Point,
    vp_size: (f32, f32),
    snapper: Snapper,
    snap_popup_open: bool,
    /// Whether Tangent snap was enabled before a tangent-pick command started.
    pre_cmd_tangent: Option<bool>,
    /// Orthogonal drawing constraint (F8): constrains picks to 0°/90°/180°/270°.
    ortho_mode: bool,
    /// Polar tracking (F10): constrains picks to configurable angle increments.
    polar_mode: bool,
    /// Polar tracking angle increment in degrees (15 / 30 / 45 / 90).
    polar_increment_deg: f32,
    /// Show grid lines in the viewport (F7).
    show_grid: bool,
    /// Dynamic input overlay (F12): show coordinate tooltip near cursor.
    dyn_input: bool,
    /// Show the UCS icon in the bottom-left corner of model space (UCSICON).
    show_ucs_icon: bool,
    /// Last point committed by a drawing command — used as ortho/polar base.
    last_point: Option<glam::Vec3>,
    /// OS window Id for the floating Layer Properties Manager (None when closed).
    layer_window: Option<window::Id>,
    /// OS window Id of the primary application window.
    main_window: Option<window::Id>,
    // ── Floating panel windows ────────────────────────────────────────────
    page_setup_window:      Option<window::Id>,
    textstyle_window:       Option<window::Id>,
    tablestyle_window:      Option<window::Id>,
    mlstyle_window:         Option<window::Id>,
    layout_manager_window:  Option<window::Id>,
    plotstyle_window:       Option<window::Id>,
    dimstyle_window:        Option<window::Id>,
    shortcuts_window:       Option<window::Id>,
    about_window:           Option<window::Id>,
    /// In-memory clipboard: cloned entities waiting to be pasted.
    clipboard: Vec<acadrust::EntityType>,
    /// Centroid of the clipboard entities (XZ plane, Y-up).
    clipboard_centroid: glam::Vec3,
    /// Which layout tab has its context menu open (None = closed).
    layout_context_menu: Option<String>,
    /// Inline rename state: (original_name, current_edit_value).
    layout_rename_state: Option<(String, String)>,
    /// Timestamp of the previous viewport left-click release (for double-click detection).
    last_vp_click_time: Option<Instant>,
    /// Screen position of the previous viewport left-click release.
    last_vp_click_pos: Option<Point>,
    // page_setup_open: moved to page_setup_window: Option<window::Id>
    /// Editable paper width buffer for the Page Setup panel (string while typing).
    page_setup_w: String,
    /// Editable paper height buffer for the Page Setup panel (string while typing).
    page_setup_h: String,
    /// Plot area type: "Layout" | "Extents".
    page_setup_plot_area: String,
    /// Center the drawing on the page when exporting.
    page_setup_center: bool,
    /// Plot offset X in mm (applied after optional centering).
    page_setup_offset_x: String,
    /// Plot offset Y in mm.
    page_setup_offset_y: String,
    /// Plot rotation in degrees: "0" | "90" | "180" | "270".
    page_setup_rotation: String,
    /// Plot scale: "Fit" | "1:1" | "1:2" | "1:4" | "1:5" | "1:10" | "1:20" | "1:50" | "1:100" | "2:1".
    page_setup_scale: String,

    // ── Plot Style Table ──────────────────────────────────────────────────
    /// Currently loaded CTB/STB table (None = no override).
    active_plot_style: Option<crate::io::plot_style::PlotStyleTable>,

    // ── MLineStyle Dialog ─────────────────────────────────────────────────
    mlstyle_selected: String,

    // ── TableStyle Dialog ─────────────────────────────────────────────────
    tablestyle_selected: String,

    // ── TextStyle Font Browser ────────────────────────────────────────────
    textstyle_selected: String,
    /// Edit buffer for font file name.
    textstyle_font: String,
    /// Edit buffer for width factor.
    textstyle_width: String,
    /// Edit buffer for oblique angle (degrees).
    textstyle_oblique: String,

    // ── Color Scheme ──────────────────────────────────────────────────────
    active_theme: Theme,

    // ── Keyboard Shortcut Editor ──────────────────────────────────────────
    /// User-defined function-key overrides: "F3" → command string.
    shortcut_overrides: std::collections::HashMap<String, String>,

    // ── Layout Manager Panel ──────────────────────────────────────────────
    layout_manager_selected: String,
    layout_manager_rename_buf: String,

    // ── Plot Style Panel ──────────────────────────────────────────────────
    /// Selected ACI index in the panel (1-255).
    plotstyle_panel_aci: u8,
    /// Edit buffers for the selected entry.
    ps_color_buf: String,
    ps_lineweight_buf: String,
    ps_screening_buf: String,

    // ── DimStyle Dialog ───────────────────────────────────────────────────
    /// Name of the style currently shown in the dialog.
    dimstyle_selected: String,
    /// Active tab: 0=Lines, 1=Arrows, 2=Text, 3=Scale/Units, 4=Tolerances.
    dimstyle_tab: u8,
    // Edit buffers (strings while typing):
    ds_dimdle: String, ds_dimdli: String, ds_dimgap: String,
    ds_dimexe: String, ds_dimexo: String,
    ds_dimsd1: bool,   ds_dimsd2: bool,
    ds_dimse1: bool,   ds_dimse2: bool,
    ds_dimasz: String, ds_dimcen: String, ds_dimtsz: String,
    ds_dimtxt: String, ds_dimtxsty: String, ds_dimtad: String,
    ds_dimtih: bool,   ds_dimtoh: bool,
    ds_dimscale: String, ds_dimlfac: String,
    ds_dimlunit: String, ds_dimdec: String, ds_dimpost: String,
    ds_dimtol: bool,   ds_dimlim: bool,
    ds_dimtp: String,  ds_dimtm: String,
    ds_dimtdec: String, ds_dimtfac: String,
}

/// Identifies a DimStyle field that can be edited in the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DsField {
    Dimdle, Dimdli, Dimgap, Dimexe, Dimexo,
    Dimsd1, Dimsd2, Dimse1, Dimse2,
    Dimasz, Dimcen, Dimtsz,
    Dimtxt, Dimtxsty, Dimtad, Dimtih, Dimtoh,
    Dimscale, Dimlfac, Dimlunit, Dimdec, Dimpost,
    Dimtol, Dimlim, Dimtp, Dimtm, Dimtdec, Dimtfac,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
    OpenFile,
    FileOpened(Result<(String, PathBuf, CadDocument), String>),
    SaveFile,
    SaveAs,
    PickedSavePath(Option<PathBuf>),
    ClearScene,
    SetWireframe(bool),
    /// Switch camera projection: true = Orthographic, false = Perspective.
    SetProjection(bool),
    /// Select a ribbon module tab by index.
    RibbonSelectTab(usize),
    /// A ribbon tool button was clicked — highlights the tool and dispatches its event.
    RibbonToolClick {
        tool_id: String,
        event: ModuleEvent,
    },
    // ── Application menu ──────────────────────────────────────────────────
    ToggleAppMenu,
    CloseAppMenu,
    /// Close the menu and immediately dispatch a CAD command.
    CloseAppMenuAndRun(String),
    AppMenuSearch(String),
    // ── Document tabs ──────────────────────────────────────────────────────
    /// Create a new empty document tab.
    TabNew,
    /// Switch to the given tab index.
    TabSwitch(usize),
    /// Close the given tab index.
    TabClose(usize),
    // ─────────────────────────────────────────────────────────────────────
    CommandInput(String),
    CommandSubmit,
    Command(String),
    /// Recall previous command in history (↑ arrow key).
    CommandHistoryPrev,
    /// Recall next command in history (↓ arrow key).
    CommandHistoryNext,
    ToggleLayers,
    LayerToggleVisible(usize),
    LayerToggleLock(usize),
    LayerToggleFreeze(usize),
    /// Toggle per-viewport freeze: (layer_index, vp_col_index)
    LayerToggleVpFreeze(usize, usize),
    LayerNew,
    LayerDelete,
    LayerSetCurrent,
    LayerSelect(usize),
    LayerRenameStart(usize),
    LayerRenameEdit(String),
    LayerColorPickerToggle(usize),
    LayerColorMorePalette,
    LayerColorSet(u8),
    LayerLinetypeSet(String),
    LayerLineweightSet(LineWeight),
    LayerTransparencyEdit(usize, String),
    LayerRenameCommit,
    CursorMoved(Point),
    ViewportClick,
    ViewportMove(Point),
    ViewportLeftPress,
    ViewportLeftRelease,
    ViewportRightPress,
    ViewportRightRelease,
    ViewportMiddlePress,
    ViewportMiddleRelease,
    ViewportScroll(mouse::ScrollDelta),
    ViewportExit,
    ViewCubeSnap(CubeRegion),
    WindowResized(f32, f32),
    /// Enter pressed globally — finalises the active command (no text-input involvement).
    CommandFinalize,
    /// Escape pressed globally — cancels the active command.
    CommandEscape,
    /// Toggle the global snap on/off (OSNAP button body click).
    ToggleSnapEnabled,
    /// Toggle grid-snap on/off — F9 / SNAP status-bar button.
    ToggleGridSnap,
    /// Toggle grid display in the viewport — F7 / GRID status-bar button.
    ToggleGrid,
    /// Toggle orthogonal drawing constraint — F8 / ORTHO status-bar button.
    ToggleOrtho,
    /// Toggle polar-angle constraint — F10 / POLAR status-bar button.
    TogglePolar,
    /// Set polar tracking angle increment (right-click POLAR button).
    SetPolarAngle(f32),
    /// Toggle dynamic input overlay (F12).
    ToggleDynInput,
    /// Toggle object snap tracking (F11).
    ToggleOTrack,
    /// Toggle an individual snap mode (from popup row click).
    ToggleSnap(crate::snap::SnapType),
    /// Open / close the OSNAP popup (▾ arrow click).
    ToggleSnapPopup,
    /// Close the OSNAP popup (click-catcher outside the panel).
    CloseSnapPopup,
    /// Enable all snap modes.
    SnapSelectAll,
    /// Disable all snap modes.
    SnapClearAll,
    /// Toggle a ribbon dropdown open/closed.
    ToggleRibbonDropdown(String),
    /// Close any open ribbon dropdown (click-catcher outside the panel).
    CloseRibbonDropdown,
    /// User selected a specific item from a ribbon dropdown.
    DropdownSelectItem {
        dropdown_id: &'static str,
        cmd: &'static str,
    },
    /// Delete key — erase all currently selected entities.
    DeleteSelected,
    Undo,
    Redo,
    UndoMany(usize),
    RedoMany(usize),
    // ── Ribbon ────────────────────────────────────────────────────────────
    /// User selected a layer from the layer combobox in the ribbon.
    RibbonLayerChanged(String),
    /// User changed the active color in the Properties toolbar.
    RibbonColorChanged(AcadColor),
    /// Toggle the full ACI palette inside the ribbon color picker.
    RibbonColorPaletteToggle,
    /// User changed the active linetype in the Properties toolbar.
    RibbonLinetypeChanged(String),
    /// User changed the active lineweight in the Properties toolbar.
    RibbonLineweightChanged(LineWeight),
    /// User selected a style from a style combobox in the ribbon.
    RibbonStyleChanged { key: crate::modules::StyleKey, name: String },

    // ── Properties panel ──────────────────────────────────────────────────
    /// User selected a layer from the layer pick_list in the Properties panel.
    PropLayerChanged(String),
    PropSelectionGroupChanged(crate::ui::properties::SelectionGroup),
    /// User picked a color from the Properties color picker.
    PropColorChanged(AcadColor),
    /// User selected a lineweight from the Properties pick_list.
    PropLwChanged(LineWeight),
    /// User selected a linetype from the linetype pick_list.
    PropLinetypeChanged(String),
    /// User toggled a boolean property (e.g. Invisible).
    PropBoolToggle(&'static str),
    /// User selected a hatch pattern from the pattern pick_list in Properties.
    PropHatchPatternChanged(String),
    /// User selected a generic choice field in the Properties panel.
    PropGeomChoiceChanged {
        field: &'static str,
        value: String,
    },
    /// User is typing in an editable geometry field (live buffer update).
    PropGeomInput {
        field: &'static str,
        value: String,
    },
    /// User committed a geometry/common field edit (Enter pressed).
    PropGeomCommit(&'static str),
    /// Toggle the inline color picker dropdown open/closed.
    PropColorPickerToggle,
    /// Toggle the full ACI colour palette expansion.
    PropColorPaletteToggle,
    /// Enter the model-space editing mode inside the given viewport (MSPACE).
    EnterViewport(acadrust::Handle),
    /// Exit MSPACE and return to paper-space editing (PSPACE).
    ExitViewport,
    /// MS command: enter MSPACE for the first available viewport.
    MspaceCommand,
    /// PS command: exit MSPACE (PSPACE).
    PspaceCommand,
    /// Switch to a named layout ("Model" or paper space layout name).
    LayoutSwitch(String),
    /// Create a new paper space layout.
    LayoutCreate,
    /// Delete the named paper space layout (Model cannot be deleted).
    LayoutDelete(String),
    /// Begin inline rename for the given layout tab.
    LayoutRenameStart(String),
    /// Live-update the rename text input buffer.
    LayoutRenameEdit(String),
    /// Commit the rename (Enter pressed in the text input).
    LayoutRenameCommit,
    /// Cancel an in-progress rename (Escape).
    LayoutRenameCancel,
    /// Open the right-click context menu for the given layout tab.
    LayoutContextMenu(String),
    /// Close the layout context menu.
    LayoutContextMenuClose,
    // ── Layout Manager Panel ────────────────────────────────────────────
    LayoutManagerOpen,
    #[allow(dead_code)]
    LayoutManagerClose,
    LayoutManagerSelect(String),
    LayoutManagerRenameBuf(String),
    LayoutManagerRenameCommit,
    LayoutManagerNew,
    LayoutManagerDelete,
    LayoutManagerMoveLeft,
    LayoutManagerMoveRight,
    LayoutManagerSetCurrent,
    /// Switch the UI color scheme.
    SetTheme(Theme),
    // ── Keyboard Shortcut Editor ────────────────────────────────────────
    ShortcutsPanelOpen,
    #[allow(dead_code)]
    ShortcutsPanelClose,
    // ── About window ────────────────────────────────────────────────────
    AboutOpen,
    AboutCopyInfo,
    /// Close the viewport right-click context menu without performing any action.
    ViewportContextMenuClose,
    /// A window was closed by the OS (e.g. the user clicked the title-bar ✕).
    OsWindowClosed(window::Id),
    /// No-op — used as a fallback when a TabEvent has no host mapping.
    Noop,
    // ── Page Setup ────────────────────────────────────────────────────────
    /// Open the Page Setup panel for the current layout.
    PageSetupOpen,
    /// Close (cancel) the Page Setup panel without applying changes.
    PageSetupClose,
    /// Live-edit of the paper width field.
    PageSetupWidthEdit(String),
    /// Live-edit of the paper height field.
    PageSetupHeightEdit(String),
    /// User selected a paper size preset (e.g. "A4 Portrait").
    PageSetupPreset(String),
    /// User changed the plot area type ("Layout" or "Extents").
    PageSetupPlotArea(String),
    /// Toggle center-on-page.
    PageSetupCenterToggle,
    /// Live-edit of plot offset X.
    PageSetupOffsetXEdit(String),
    /// Live-edit of plot offset Y.
    PageSetupOffsetYEdit(String),
    /// User changed plot rotation.
    PageSetupRotation(String),
    PageSetupScale(String),
    /// Apply the changes entered in Page Setup.
    PageSetupCommit,
    // ── Plot / Export ─────────────────────────────────────────────────────
    /// Show the SVG save-file dialog and trigger export.
    PlotExport,
    /// Callback after the user picks (or cancels) the export path.
    PlotExportPath(Option<std::path::PathBuf>),
    /// Send current layout to the system printer (via lp / lpr).
    PrintToPrinter,
    /// Callback from the async printer job.
    PrintResult(Result<String, String>),
    // ── Plot Style Table ─────────────────────────────────────────────────
    /// Open file dialog to load a CTB/STB plot style table.
    PlotStyleLoad,
    /// Callback when the user picks (or cancels) a CTB/STB file.
    PlotStyleLoaded(Option<crate::io::plot_style::PlotStyleTable>),
    /// Clear the active plot style table.
    PlotStyleClear,
    /// Open/close the Plot Style panel.
    PlotStylePanelOpen,
    #[allow(dead_code)]
    PlotStylePanelClose,
    /// Select an ACI entry in the panel.
    PlotStylePanelSelectAci(u8),
    /// Edit buffers changed.
    PlotStylePanelColorBuf(String),
    PlotStylePanelLwBuf(String),
    PlotStylePanelScreenBuf(String),
    /// Apply current edit buffers to the selected ACI entry.
    PlotStylePanelApply,
    /// Save the modified table back to disk.
    PlotStylePanelSave,
    /// Save callback.
    PlotStylePanelSavePath(Option<std::path::PathBuf>),
    // ── TextStyle Font Browser ────────────────────────────────────────────
    TextStyleDialogOpen,
    #[allow(dead_code)]
    TextStyleDialogClose,
    TextStyleDialogSelect(String),
    TextStyleDialogSetCurrent,
    TextStyleDialogNew,
    TextStyleDialogDelete,
    /// Edit a string field (FontFile / Width / Oblique).
    TextStyleEdit { field: &'static str, value: String },
    /// Commit edits to the selected text style.
    TextStyleApply,
    /// Select a font from the built-in font list.
    TextStyleFontPick(String),
    // ── TableStyle Dialog ─────────────────────────────────────────────────
    TableStyleDialogOpen,
    #[allow(dead_code)]
    TableStyleDialogClose,
    TableStyleDialogSelect(String),
    TableStyleDialogNew,
    TableStyleDialogDelete,
    // ── MLineStyle Dialog ─────────────────────────────────────────────────
    MlStyleDialogOpen,
    #[allow(dead_code)]
    MlStyleDialogClose,
    MlStyleDialogSelect(String),
    MlStyleDialogSetCurrent,
    MlStyleDialogNew,
    MlStyleDialogDelete,
    // ── DimStyle Dialog ───────────────────────────────────────────────────
    DimStyleDialogOpen,
    DimStyleDialogClose,
    /// Apply edits to the selected style.
    DimStyleDialogApply,
    /// Select a different style in the dialog list.
    DimStyleDialogSelect(String),
    /// Switch the active tab.
    DimStyleDialogTab(u8),
    /// Create a new empty style (prompts via command line).
    DimStyleDialogNew,
    /// Set the selected style as the document's current dim style.
    DimStyleDialogSetCurrent,
    /// Delete the selected style.
    DimStyleDialogDelete,
    // Field edit messages:
    DsEdit(DsField, String),
    DsToggle(DsField),
    // ── Raster Image ──────────────────────────────────────────────────────
    /// Open file-picker dialog for IMAGE command (async).
    ImagePick,
    /// Result of the image file picker + pixel dimension decode.
    ImagePickResult(Result<(std::path::PathBuf, u32, u32), String>),
    // ── XREF ──────────────────────────────────────────────────────────────
    /// Open file-picker dialog for XATTACH command (async).
    XAttachPick,
    /// Result of the XATTACH file picker.
    XAttachPickResult(Result<std::path::PathBuf, String>),
    // ── WBLOCK ────────────────────────────────────────────────────────────
    /// Trigger the WBLOCK save dialog for `block_name` (or `*` = selection).
    WblockSave(String),
    /// Result of the WBLOCK save path dialog.
    WblockSaveResult(String, Option<std::path::PathBuf>),
    // ── DATAEXTRACTION ────────────────────────────────────────────────────
    /// Save the pre-built CSV string to a file chosen by the user.
    DataExtractionSave(String),
    /// Path chosen (or None = cancelled).
    DataExtractionSaveResult(String, Option<std::path::PathBuf>),
    // ── STL export ────────────────────────────────────────────────────────
    /// Trigger STL export: collect meshes and show save dialog.
    StlExport,
    /// Callback after the user picks (or cancels) the STL save path.
    StlExportPath(Option<std::path::PathBuf>),
    // ── STEP export ───────────────────────────────────────────────────────
    /// Trigger STEP AP203 export: show save dialog.
    StepExport,
    /// Callback after the user picks (or cancels) the STEP save path.
    StepExportPath(Option<std::path::PathBuf>),
    // ── OBJ import ────────────────────────────────────────────────────────
    /// Trigger OBJ import: show open-file dialog.
    ObjImport,
    /// Callback after the user picks (or cancels) the OBJ file path.
    ObjImportPath(Option<std::path::PathBuf>),
}

impl H7CAD {
    fn new() -> Self {
        let first_tab = DocumentTab::new_drawing(1);
        let mut app = Self {
            start: Instant::now(),
            tabs: vec![first_tab],
            active_tab: 0,
            tab_counter: 1,
            ribbon: Ribbon::new(),
            app_menu: AppMenu::new(),
            command_line: CommandLine::new(),
            status_bar: StatusBar::new(),
            cursor_pos: Point::ORIGIN,
            vp_size: (1280.0, 720.0),
            snapper: Snapper::default(),
            snap_popup_open: false,
            pre_cmd_tangent: None,
            ortho_mode: false,
            polar_mode: false,
            polar_increment_deg: 45.0,
            show_grid: false,
            dyn_input: true,
            show_ucs_icon: true,
            last_point: None,
            layer_window: None,
            main_window: None,
            page_setup_window:     None,
            textstyle_window:      None,
            tablestyle_window:     None,
            mlstyle_window:        None,
            layout_manager_window: None,
            plotstyle_window:      None,
            dimstyle_window:       None,
            shortcuts_window:      None,
            about_window:          None,
            clipboard: Vec::new(),
            clipboard_centroid: glam::Vec3::ZERO,
            layout_context_menu: None,
            layout_rename_state: None,
            last_vp_click_time: None,
            last_vp_click_pos: None,
            page_setup_w: String::new(),
            page_setup_h: String::new(),
            page_setup_plot_area: "Layout".to_string(),
            page_setup_center: true,
            page_setup_offset_x: "0.0".to_string(),
            page_setup_offset_y: "0.0".to_string(),
            page_setup_rotation: "0".to_string(),
            page_setup_scale: "Fit".to_string(),
            // Plot style
            active_plot_style: None,
            // Color scheme (default: dark CAD-style)
            active_theme: Theme::Dark,
            // Keyboard shortcuts
            shortcut_overrides: std::collections::HashMap::new(),
            // Layout Manager
            layout_manager_selected: "Model".to_string(),
            layout_manager_rename_buf: String::new(),
            plotstyle_panel_aci: 1,
            ps_color_buf: String::new(),
            ps_lineweight_buf: "255".to_string(),
            ps_screening_buf: "100".to_string(),
            // TextStyle font browser
            textstyle_selected: "Standard".to_string(),
            textstyle_font: String::new(),
            textstyle_width: "1.0".to_string(),
            textstyle_oblique: "0.0".to_string(),
            // TableStyle dialog
            tablestyle_selected: "Standard".to_string(),
            // MLineStyle dialog
            mlstyle_selected: "Standard".to_string(),
            // DimStyle dialog
            dimstyle_selected: "Standard".to_string(),
            dimstyle_tab: 0,
            ds_dimdle: "0".to_string(),       ds_dimdli: "3.75".to_string(),
            ds_dimgap: "0.625".to_string(),   ds_dimexe: "1.25".to_string(),
            ds_dimexo: "0.625".to_string(),
            ds_dimsd1: false, ds_dimsd2: false,
            ds_dimse1: false, ds_dimse2: false,
            ds_dimasz: "0.18".to_string(),    ds_dimcen: "0.09".to_string(),
            ds_dimtsz: "0".to_string(),
            ds_dimtxt: "0.18".to_string(),    ds_dimtxsty: "Standard".to_string(),
            ds_dimtad: "1".to_string(),
            ds_dimtih: false, ds_dimtoh: false,
            ds_dimscale: "1".to_string(),     ds_dimlfac: "1".to_string(),
            ds_dimlunit: "2".to_string(),     ds_dimdec: "2".to_string(),
            ds_dimpost: "<>".to_string(),
            ds_dimtol: false, ds_dimlim: false,
            ds_dimtp: "0".to_string(),        ds_dimtm: "0".to_string(),
            ds_dimtdec: "2".to_string(),      ds_dimtfac: "1".to_string(),
        };
        app.sync_ribbon_layers();
        app
    }

    /// Boot function for `iced::daemon`: returns initial state plus a task that
    /// opens the primary application window.
    fn boot() -> (Self, Task<Message>) {
        use helpers::build_window_icon;
        let state = Self::new();
        let (id, open_task) = window::open(window::Settings {
            maximized: true,
            icon: window::icon::from_rgba(build_window_icon(), 32, 32).ok(),
            ..Default::default()
        });
        let mut s = state;
        s.main_window = Some(id);
        let task = open_task.map(|_| Message::Noop);
        (s, task)
    }
}

use std::path::PathBuf;

pub fn run() -> iced::Result {
    iced::daemon(H7CAD::boot, H7CAD::update, H7CAD::view)
        .subscription(H7CAD::subscription)
        .title(|state: &H7CAD, window_id: window::Id| {
            if Some(window_id) == state.layer_window         { return "Layer Properties Manager".into(); }
            if Some(window_id) == state.page_setup_window    { return "Page Setup".into(); }
            if Some(window_id) == state.textstyle_window     { return "Text Style".into(); }
            if Some(window_id) == state.tablestyle_window    { return "Table Style".into(); }
            if Some(window_id) == state.mlstyle_window       { return "Multiline Style".into(); }
            if Some(window_id) == state.layout_manager_window { return "Layout Manager".into(); }
            if Some(window_id) == state.plotstyle_window     { return "Plot Style Table Editor".into(); }
            if Some(window_id) == state.dimstyle_window      { return "Dimension Style Manager".into(); }
            if Some(window_id) == state.shortcuts_window     { return "Keyboard Shortcuts".into(); }
            if Some(window_id) == state.about_window         { return "About H7CAD".into(); }
            if let Some(tab) = state.tabs.get(state.active_tab) {
                let dot = if tab.dirty { "● " } else { "" };
                let name = tab.tab_display_name();
                format!("{}H7CAD — {}", dot, name)
            } else {
                "H7CAD".to_string()
            }
        })
        .theme(|state: &H7CAD, _| state.active_theme.clone())
        .run()
}
