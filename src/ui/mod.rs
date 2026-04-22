/// Single source of truth for UI row height (px).
/// Change this to scale the ribbon, layer manager rows, and property panel rows uniformly.
pub const ROW_H: f32 = 26.0;

pub mod about;
pub mod app_menu;
pub mod command_line;
pub mod dimstyle;
pub mod layers;
pub mod layout_manager;
pub mod mlstyle;
pub mod overlay;
pub mod page_setup;
pub mod plotstyle;
pub mod properties;
pub mod ribbon;
pub mod shortcuts;
pub mod snap_popup;
pub mod statusbar;
pub mod tablestyle;
pub mod textstyle;

pub use app_menu::AppMenu;
pub use command_line::CommandLine;
pub use layers::LayerPanel;
pub use properties::PropertiesPanel;
pub use ribbon::Ribbon;
pub use statusbar::StatusBar;
