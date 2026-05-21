//! Application menu — H7CAD-style dropdown that opens when the H7 logo
//! button is clicked. Overlays the entire viewport as a floating panel.
//!
//! Layout:
//!   ┌─────────────────────────────────────────────┐
//!   │  [Search bar]                               │
//!   ├──────────────────┬──────────────────────────┤
//!   │  Command list    │  Recent files            │
//!   │  (left column)   │  (right column)          │
//!   └──────────────────┴──────────────────────────┘

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Background, Border, Color, Element, Length, Theme};
use std::path::PathBuf;

use crate::app::Message;

// ── Menu item definitions ─────────────────────────────────────────────────

struct MenuItem {
    icon: &'static str,
    label: &'static str,
    desc: &'static str,
    command: &'static str,
}

const MENU_ITEMS: &[MenuItem] = &[
    MenuItem {
        icon: "◻",
        label: "New",
        desc: "Create a new drawing",
        command: "NEW",
    },
    MenuItem {
        icon: "📂",
        label: "Open",
        desc: "Open an existing file",
        command: "OPEN",
    },
    MenuItem {
        icon: "💾",
        label: "Save",
        desc: "Save the current drawing",
        command: "SAVE",
    },
    MenuItem {
        icon: "📄",
        label: "Save As",
        desc: "Save as DWG or DXF",
        command: "SAVEAS",
    },
    MenuItem {
        icon: "🖨",
        label: "Print",
        desc: "Print or plot the drawing",
        command: "PRINT",
    },
    MenuItem {
        icon: "⚙",
        label: "Options",
        desc: "Open application settings",
        command: "OPTIONS",
    },
    MenuItem {
        icon: "❓",
        label: "Help",
        desc: "Open help documentation",
        command: "HELP",
    },
];

// ── Public state ──────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct AppMenu {
    pub open: bool,
    pub search: String,
    /// Recently opened file paths (most recent first).
    pub recent: Vec<PathBuf>,
}

impl AppMenu {
    pub fn new() -> Self {
        Self {
            open: false,
            search: String::new(),
            recent: vec![],
        }
    }

    /// Call when a file is successfully loaded to push its path into recents.
    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|r| r != &path);
        self.recent.insert(0, path);
        self.recent.truncate(20);
        // Best-effort persist; silent on failure (read-only home, full disk).
        let _ = save_recents(&self.recent);
    }

    /// Drop a path from the recents list (manual removal from Start page).
    pub fn remove_recent(&mut self, path: &std::path::Path) {
        self.recent.retain(|r| r.as_path() != path);
        let _ = save_recents(&self.recent);
    }

    /// Rehydrate the recents list from disk. Call once at app boot.
    pub fn load_persistent_recents(&mut self) {
        self.recent = load_recents();
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.search.clear();
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.search.clear();
    }

    // ── View ─────────────────────────────────────────────────────────────

    pub fn view(&self) -> Element<'_, Message> {
        if !self.open {
            return container(text("")).width(0).height(0).into();
        }

        // Filter items by search query.
        let query = self.search.to_lowercase();
        let items: Vec<&MenuItem> = MENU_ITEMS
            .iter()
            .filter(|m| {
                query.is_empty()
                    || m.label.to_lowercase().contains(&query)
                    || m.desc.to_lowercase().contains(&query)
            })
            .collect();

        // ── Search bar ────────────────────────────────────────────────────
        let search = container(
            text_input("Search commands...", &self.search)
                .on_input(Message::AppMenuSearch)
                .style(|_: &Theme, _| iced::widget::text_input::Style {
                    background: Background::Color(SEARCH_BG),
                    border: Border {
                        color: ACCENT,
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    icon: Color::WHITE,
                    placeholder: Color {
                        r: 0.45,
                        g: 0.45,
                        b: 0.45,
                        a: 1.0,
                    },
                    value: Color::WHITE,
                    selection: Color {
                        r: 0.20,
                        g: 0.44,
                        b: 0.72,
                        a: 0.5,
                    },
                })
                .size(12)
                .padding([6, 10]),
        )
        .padding([8, 10])
        .width(Length::Fill)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(HEADER_BG)),
            border: Border {
                color: BORDER,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        // ── Left column — command list ─────────────────────────────────────
        let left: Element<'_, Message> = if items.is_empty() {
            container(text("No results").size(11).color(DIM_COLOR))
                .padding([12, 14])
                .into()
        } else {
            items
                .iter()
                .fold(column![].spacing(0), |col, item| {
                    col.push(menu_item_btn(item))
                })
                .into()
        };

        let left_col = container(scrollable(left))
            .width(220)
            .height(Length::Fill)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(LEFT_BG)),
                ..Default::default()
            });

        // ── Right column — recent files ───────────────────────────────────
        let recent_header =
            container(text("Recent Documents").size(10).color(DIM_COLOR)).padding(iced::Padding {
                top: 10.0,
                right: 14.0,
                bottom: 6.0,
                left: 14.0,
            });

        let recent_list = if self.recent.is_empty() {
            column![container(text("No recent files").size(11).color(DIM_COLOR)).padding([8, 14])]
        } else {
            self.recent.iter().fold(column![].spacing(0), |col, path| {
                col.push(recent_item_btn(path))
            })
        };

        let right_col = container(column![recent_header, scrollable(recent_list)])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(RIGHT_BG)),
                border: Border {
                    color: BORDER,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

        // ── Bottom bar — Exit button ───────────────────────────────────────
        let bottom = container(
            row![
                iced::widget::Space::new().width(Length::Fill),
                button(text("Exit H7CAD").size(11).color(Color::WHITE))
                    .on_press(Message::Command("EXIT".into()))
                    .style(|_: &Theme, status| button::Style {
                        background: Some(Background::Color(match status {
                            button::Status::Hovered => Color {
                                r: 0.65,
                                g: 0.12,
                                b: 0.12,
                                a: 1.0
                            },
                            button::Status::Pressed => Color {
                                r: 0.45,
                                g: 0.08,
                                b: 0.08,
                                a: 1.0
                            },
                            _ => EXIT_BTN,
                        })),
                        text_color: Color::WHITE,
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: 2.0.into()
                        },
                        shadow: iced::Shadow::default(),
                        snap: false,
                    })
                    .padding([5, 14]),
            ]
            .align_y(iced::Center),
        )
        .padding([6, 10])
        .width(Length::Fill)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BOTTOM_BG)),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        // ── Outer panel ───────────────────────────────────────────────────
        let panel = container(column![
            search,
            container(row![left_col, right_col]).height(Length::Fill),
            bottom,
        ])
        .width(520)
        .height(420)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(PANEL_BG)),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        // Backdrop — clicking outside closes the menu.
        let backdrop = button(
            container(panel)
                // Offset so the panel sits just below the ribbon H7 button.
                .padding(iced::Padding {
                    top: 56.0,
                    left: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                })
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(Message::CloseAppMenu)
        .style(|_: &Theme, _| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.55,
            })),
            text_color: Color::TRANSPARENT,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        backdrop
    }
}

// ── Item widgets ──────────────────────────────────────────────────────────

fn menu_item_btn<'a>(item: &'a MenuItem) -> Element<'a, Message> {
    button(
        row![
            container(text(item.icon).size(16).color(ICON_COLOR))
                .width(32)
                .padding([0, 4]),
            column![
                text(item.label).size(12).color(Color::WHITE),
                text(item.desc).size(10).color(DIM_COLOR),
            ]
            .spacing(1),
        ]
        .align_y(iced::Center)
        .spacing(4),
    )
    .on_press(Message::CloseAppMenuAndRun(item.command.into()))
    .style(|_: &Theme, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => ITEM_HOVER,
            button::Status::Pressed => ITEM_ACTIVE,
            _ => Color::TRANSPARENT,
        })),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .padding([7, 10])
    .width(Length::Fill)
    .into()
}

fn recent_item_btn(path: &PathBuf) -> Element<'_, Message> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let type_label = match path
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .as_deref()
    {
        Some("DWG") => "DWG Drawing",
        Some("DXF") => "DXF Drawing",
        _ => "CAD File",
    };
    let path_str = path.display().to_string();
    button(
        row![
            container(text("📄").size(13).color(ICON_COLOR)).width(24),
            column![
                text(name).size(11).color(Color::WHITE),
                text(type_label).size(9).color(DIM_COLOR),
            ]
            .spacing(1),
        ]
        .align_y(iced::Center)
        .spacing(6),
    )
    .on_press(Message::Command(format!("OPEN_RECENT:{path_str}")))
    .style(|_: &Theme, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => ITEM_HOVER,
            button::Status::Pressed => ITEM_ACTIVE,
            _ => Color::TRANSPARENT,
        })),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .padding([6, 14])
    .width(Length::Fill)
    .into()
}

// ── Colors ────────────────────────────────────────────────────────────────

const PANEL_BG: Color = Color {
    r: 0.20,
    g: 0.20,
    b: 0.20,
    a: 1.0,
};
const HEADER_BG: Color = Color {
    r: 0.17,
    g: 0.17,
    b: 0.17,
    a: 1.0,
};
const LEFT_BG: Color = Color {
    r: 0.22,
    g: 0.22,
    b: 0.22,
    a: 1.0,
};
const RIGHT_BG: Color = Color {
    r: 0.18,
    g: 0.18,
    b: 0.18,
    a: 1.0,
};
const BOTTOM_BG: Color = Color {
    r: 0.16,
    g: 0.16,
    b: 0.16,
    a: 1.0,
};
const SEARCH_BG: Color = Color {
    r: 0.14,
    g: 0.14,
    b: 0.14,
    a: 1.0,
};
const BORDER: Color = Color {
    r: 0.30,
    g: 0.30,
    b: 0.30,
    a: 1.0,
};
const ITEM_HOVER: Color = Color {
    r: 0.28,
    g: 0.28,
    b: 0.28,
    a: 1.0,
};
const ITEM_ACTIVE: Color = Color {
    r: 0.18,
    g: 0.42,
    b: 0.70,
    a: 1.0,
};
const DIM_COLOR: Color = Color {
    r: 0.50,
    g: 0.50,
    b: 0.50,
    a: 1.0,
};
const ICON_COLOR: Color = Color {
    r: 0.70,
    g: 0.80,
    b: 0.95,
    a: 1.0,
};
const ACCENT: Color = Color {
    r: 0.20,
    g: 0.55,
    b: 0.90,
    a: 1.0,
};
const EXIT_BTN: Color = Color {
    r: 0.55,
    g: 0.10,
    b: 0.10,
    a: 1.0,
};

// ── Recent-files persistence ─────────────────────────────────────────────
//
// Plain-text format, one path per line, newest first. Lives next to other
// per-user config so we don't pull in a TOML/JSON crate just for this.

fn recents_file_path() -> Option<PathBuf> {
    let base: PathBuf = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        let mut p = PathBuf::from(home);
        p.push("Library");
        p.push("Application Support");
        p
    } else if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(d)
    } else {
        let home = std::env::var_os("HOME")?;
        let mut p = PathBuf::from(home);
        p.push(".config");
        p
    };
    let mut p = base;
    p.push("H7CAD");
    Some(p.join("recent.txt"))
}

fn save_recents(list: &[PathBuf]) -> std::io::Result<()> {
    let Some(path) = recents_file_path() else { return Ok(()); };
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    let body: String = list
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path, body)
}

fn load_recents() -> Vec<PathBuf> {
    let Some(path) = recents_file_path() else { return vec![]; };
    let Ok(body) = std::fs::read_to_string(path) else { return vec![]; };
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}
