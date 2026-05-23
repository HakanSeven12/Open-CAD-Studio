//! Bottom status bar — Model/Layout tabs + OSNAP toggle + status info

use iced::widget::tooltip::Position as TipPos;
use iced::widget::{button, container, mouse_area, row, text, text_input, tooltip, Row};
use iced::{Background, Border, Color, Element, Length, Theme};

use crate::app::Message;
use crate::snap::Snapper;

#[derive(Clone, Default)]
pub struct StatusBar {
    #[allow(dead_code)]
    pub coord_display: String,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            coord_display: "MODEL".into(),
        }
    }

    pub fn view<'a>(
        &'a self,
        snapper: &'a Snapper,
        popup_open: bool,
        ortho_mode: bool,
        polar_mode: bool,
        polar_increment_deg: f32,
        show_grid: bool,
        dyn_input: bool,
        otrack: bool,
        layouts: Vec<String>,
        current_layout: String,
        // If `Some((original, edit_value))`, the named tab shows a text input.
        rename_state: Option<&'a (String, String)>,
        // Scale of the first user viewport in the active paper layout.
        viewport_scale: Option<f64>,
        // Number of user viewports in the current paper layout (0 = model space).
        viewport_count: usize,
        // True when the user is editing inside a paper-space viewport (MSPACE).
        in_mspace: bool,
        // Whether the layout tabs (Model/Paper) are visible (LAYOUTTAB).
        show_layout_tabs: bool,
        // Current annotation scale for model space (1.0 = 1:1, 50.0 = 1:50, etc.).
        annotation_scale: f32,
        // True when the scale picker popup is open.
        scale_popup_open: bool,
        // True when the scale pill is interactive (always model space; paper space only when a viewport is active/selected).
        scale_pill_enabled: bool,
        // LWDISPLAY header flag — controls lineweight visibility in the viewport.
        lineweight_display: bool,
    ) -> Element<'a, Message> {
        let menu_btn = button(text("≡").size(14).color(ICON_COLOR))
            .on_press(Message::Command("MENU".into()))
            .style(|_: &Theme, _| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            })
            .padding([2, 8]);

        let add_btn = button(text("+").size(12).color(ICON_COLOR))
            .on_press(Message::LayoutCreate)
            .style(|_: &Theme, _| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            })
            .padding([3, 7]);

        // ── Right side ────────────────────────────────────────────────────
        let osnap_active = snapper.is_active();
        let snap_grid_on = snapper.is_on(crate::snap::SnapType::Grid) && snapper.snap_enabled;

        let vp_label = if viewport_count > 0 {
            format!("{} VP", viewport_count)
        } else {
            String::new()
        };
        // Scale pill: opens the scale picker popup.
        // Model space: always interactive, shows annotation scale.
        // Paper space: interactive only when a viewport is active/selected.
        let scale_label = if current_layout == "Model" {
            format_scale(Some(1.0 / annotation_scale as f64))
        } else {
            format_scale(viewport_scale)
        };
        let scale_element: Element<'_, Message> = if scale_pill_enabled {
            tip(
                scale_popup_btn(&scale_label, scale_popup_open),
                "Annotation / Viewport Scale\nClick to change",
            )
        } else {
            status_pill(scale_label).into()
        };
        let mut right_status = row![
            tip(
                toggle_pill("SNAP", snap_grid_on, Message::ToggleGridSnap),
                "Snap to Grid\nF9"
            ),
            tip(
                toggle_pill("GRID", show_grid, Message::ToggleGrid),
                "Show Grid\nF7"
            ),
            tip(
                toggle_pill("ORTHO", ortho_mode, Message::ToggleOrtho),
                "Orthogonal Mode\nF8"
            ),
            tip(
                toggle_pill("LWT", lineweight_display, Message::ToggleLineweightDisplay),
                "Show Lineweight\nLWDISPLAY"
            ),
            polar_pill(polar_mode, polar_increment_deg),
            tip(
                toggle_pill("DYN", dyn_input, Message::ToggleDynInput),
                "Dynamic Input\nF12"
            ),
            tip(
                toggle_pill("OTRACK", otrack, Message::ToggleOTrack),
                "Object Snap Tracking\nF11"
            ),
            osnap_btn(osnap_active, snapper.snap_enabled, popup_open),
            tip(
                space_mode_btn(&current_layout, in_mspace),
                "PAPER: double-click viewport to enter MSPACE\nMODEL: click to switch to Model Space",
            ),
            scale_element,
        ]
        .spacing(2);
        if !vp_label.is_empty() {
            right_status = right_status.push(tip(
                status_pill(vp_label).into(),
                "Viewport count in active layout",
            ));
        }
        let right_status = right_status;

        let mut bar = Row::new().align_y(iced::Center).spacing(0);
        bar = bar.push(menu_btn);
        if show_layout_tabs {
            for name in layouts {
                let is_active = name == current_layout;
                let renaming = rename_state
                    .filter(|(orig, _)| *orig == name)
                    .map(|(_, edit)| edit.as_str());
                bar = bar.push(space_tab(name, is_active, renaming));
            }
            bar = bar.push(add_btn);
        }
        bar = bar.push(iced::widget::Space::new().width(Length::Fill));
        bar = bar.push(right_status);

        container(bar)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(BAR_BG)),
                border: Border {
                    color: BORDER_COLOR,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .width(Length::Fill)
            .height(26)
            .padding([0, 4])
            .into()
    }
}

// ── Tooltip helper ────────────────────────────────────────────────────────

fn tip<'a>(content: Element<'a, Message>, label: &'static str) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(label).size(11).color(Color::WHITE))
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.13,
                    g: 0.13,
                    b: 0.13,
                    a: 0.97,
                })),
                border: Border {
                    color: Color {
                        r: 0.35,
                        g: 0.35,
                        b: 0.35,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                text_color: Some(Color::WHITE),
                ..Default::default()
            })
            .padding([4, 8]),
        TipPos::Top,
    )
    .into()
}

// ── Simple toggle pill ────────────────────────────────────────────────────

fn toggle_pill(label: &'static str, active: bool, msg: Message) -> Element<'static, Message> {
    button(text(label).size(10).color(if active {
        OSNAP_ON_TEXT
    } else {
        OSNAP_OFF_TEXT
    }))
    .on_press(msg)
    .style(move |_: &Theme, status| button::Style {
        background: Some(Background::Color(match (active, status) {
            (true, button::Status::Hovered) => SNAP_ON_HOVER,
            (true, _) => SNAP_ON_BG,
            (false, button::Status::Hovered) => SNAP_OFF_HOVER,
            (false, _) => SNAP_OFF_BG,
        })),
        border: Border {
            color: if active { SNAP_BORDER_ON } else { BORDER_COLOR },
            width: 1.0,
            radius: 2.0.into(),
        },
        text_color: if active {
            OSNAP_ON_TEXT
        } else {
            OSNAP_OFF_TEXT
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .padding([2, 6])
    .into()
}

// ── Polar tracking pill ───────────────────────────────────────────────────
//
// Left-click toggles polar on/off.
// Right-click cycles through common angle increments: 15 → 30 → 45 → 90 → 15 …

fn polar_pill(active: bool, increment_deg: f32) -> Element<'static, Message> {
    let label = format!("POLAR {:.0}°", increment_deg);
    let tooltip_text = format!(
        "Polar Tracking ({}°)\nF10 — left-click on/off\nRight-click to change angle",
        increment_deg as u32
    );

    let bg_color = move |hovered: bool| match (active, hovered) {
        (true, true) => SNAP_ON_HOVER,
        (true, false) => SNAP_ON_BG,
        (false, true) => SNAP_OFF_HOVER,
        (false, false) => SNAP_OFF_BG,
    };

    // Cycle to the next common angle on right-click.
    let next_angle = match increment_deg as u32 {
        15 => 30.0_f32,
        30 => 45.0,
        45 => 90.0,
        _ => 15.0,
    };

    let inner = container(text(label).size(10).color(if active {
        OSNAP_ON_TEXT
    } else {
        OSNAP_OFF_TEXT
    }))
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(bg_color(false))),
        border: Border {
            color: if active { SNAP_BORDER_ON } else { BORDER_COLOR },
            width: 1.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    })
    .padding([2, 6]);

    let pill = mouse_area(inner)
        .on_press(Message::TogglePolar)
        .on_right_press(Message::SetPolarAngle(next_angle));

    tooltip(
        pill,
        container(text(tooltip_text).size(11).color(Color::WHITE))
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.13,
                    g: 0.13,
                    b: 0.13,
                    a: 0.95,
                })),
                border: Border {
                    color: Color {
                        r: 0.35,
                        g: 0.35,
                        b: 0.35,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            })
            .padding([4, 8]),
        TipPos::Top,
    )
    .into()
}

// ── OSNAP split button ────────────────────────────────────────────────────
//
// Left part  ("⚡ OSNAP"): toggles the global snap on/off.
// Right part ("▾"):        opens the snap-type dropdown.

fn osnap_btn(active: bool, snap_enabled: bool, open: bool) -> Element<'static, Message> {
    let bg = match (active || snap_enabled, open) {
        (true, true) => SNAP_ON_HOVER,
        (true, false) => SNAP_ON_BG,
        (false, _) => SNAP_OFF_BG,
    };
    let border_color = if open {
        ACCENT
    } else if active {
        SNAP_BORDER_ON
    } else {
        BORDER_COLOR
    };
    let text_color = if active {
        OSNAP_ON_TEXT
    } else {
        OSNAP_OFF_TEXT
    };

    let left = button(text("⚡ OSNAP").size(10).color(text_color))
        .on_press(Message::ToggleSnapEnabled)
        .style(move |_: &Theme, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered => {
                    if active || snap_enabled {
                        SNAP_ON_HOVER
                    } else {
                        SNAP_OFF_HOVER
                    }
                }
                _ => bg,
            })),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: iced::border::Radius {
                    top_left: 2.0,
                    top_right: 0.0,
                    bottom_right: 0.0,
                    bottom_left: 2.0,
                },
            },
            text_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .padding([2, 6]);

    let right = button(text("▾").size(9).color(text_color))
        .on_press(Message::ToggleSnapPopup)
        .style(move |_: &Theme, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered => {
                    if active || snap_enabled {
                        SNAP_ON_HOVER
                    } else {
                        SNAP_OFF_HOVER
                    }
                }
                _ => bg,
            })),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: iced::border::Radius {
                    top_left: 0.0,
                    top_right: 2.0,
                    bottom_right: 2.0,
                    bottom_left: 0.0,
                },
            },
            text_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .padding([2, 4]);

    row![
        tip(left.into(), "Object Snap: toggle on/off\nF3"),
        tip(right.into(), "Object Snap settings\nClick ▾"),
    ]
    .spacing(0)
    .into()
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// A layout tab button.
///
/// When `rename_edit` is `Some(value)` the tab shows an inline text input
/// instead of the normal button.  The tab is not renameable when it is the
/// "Model" tab (callers simply never pass `Some` for that name).
fn space_tab<'a>(
    label: String,
    is_active: bool,
    rename_edit: Option<&'a str>,
) -> Element<'a, Message> {
    let bg = move |is_active: bool, hovered: bool| {
        if is_active {
            TAB_ACTIVE
        } else if hovered {
            TAB_HOVER
        } else {
            Color::TRANSPARENT
        }
    };

    let border = Border {
        color: if is_active {
            ACCENT
        } else {
            Color::TRANSPARENT
        },
        width: if is_active { 1.0 } else { 0.0 },
        radius: 2.0.into(),
    };

    let text_color = if is_active {
        Color::WHITE
    } else {
        Color {
            r: 0.65,
            g: 0.65,
            b: 0.65,
            a: 1.0,
        }
    };

    if let Some(edit_val) = rename_edit {
        // Inline rename text input with a cancel (✕) button.
        let input = text_input("", edit_val)
            .on_input(Message::LayoutRenameEdit)
            .on_submit(Message::LayoutRenameCommit)
            .size(11)
            .style(|_: &Theme, _| text_input::Style {
                background: Background::Color(TAB_ACTIVE),
                border: Border {
                    color: ACCENT,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                icon: Color::WHITE,
                placeholder: Color {
                    r: 0.5,
                    g: 0.5,
                    b: 0.5,
                    a: 1.0,
                },
                value: Color::WHITE,
                selection: Color {
                    r: 0.20,
                    g: 0.55,
                    b: 0.90,
                    a: 0.4,
                },
            })
            .padding([2, 6])
            .width(Length::Fixed(90.0));

        let cancel_btn = button(text("✕").size(10).color(Color {
            r: 0.65,
            g: 0.65,
            b: 0.65,
            a: 1.0,
        }))
        .on_press(Message::LayoutRenameCancel)
        .style(|_: &Theme, _| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
            ..Default::default()
        })
        .padding([2, 4]);

        row![input, cancel_btn]
            .spacing(0)
            .align_y(iced::Center)
            .into()
    } else {
        // Normal clickable tab — left click switches, right click opens context menu.
        let display = container(text(label.clone()).size(11).color(text_color))
            .style(move |_: &Theme| container::Style {
                background: Some(Background::Color(bg(is_active, false))),
                border,
                ..Default::default()
            })
            .padding([3, 10]);

        let switch_msg = Message::LayoutSwitch(label.clone());
        let ctx_msg = Message::LayoutContextMenu(label.clone());

        // Use mouse_area so we can capture right-click for the context menu.
        mouse_area(display)
            .on_press(switch_msg)
            .on_right_press(ctx_msg)
            .into()
    }
}

/// Space-mode toggle button in the status bar.
///
/// - Model tab            → "MODEL"  (non-clickable, informational)
/// - Layout, PSPACE       → "PAPER"  (click → MspaceCommand: enter MSPACE)
/// - Layout, MSPACE       → "MODEL"  (click → ExitViewport: return to PSPACE)
fn space_mode_btn(current_layout: &str, in_mspace: bool) -> Element<'static, Message> {
    let is_model_tab = current_layout == "Model";

    // Labels and styling follow AutoCAD convention:
    //   PAPER = currently in paper-space editing
    //   MODEL = currently in model-space editing (either the Model tab or MSPACE)
    let (label, active, on_press) = if is_model_tab {
        ("MODEL", false, None::<Message>)
    } else if in_mspace {
        ("MODEL", true, Some(Message::ExitViewport))
    } else {
        ("PAPER", false, Some(Message::MspaceCommand))
    };

    let text_color = if active {
        SNAP_BORDER_ON
    } else {
        OSNAP_OFF_TEXT
    };
    let bg_normal = if active { SNAP_ON_BG } else { SNAP_OFF_BG };
    let bg_hover = if active {
        SNAP_ON_HOVER
    } else {
        SNAP_OFF_HOVER
    };
    let border_color = if active { SNAP_BORDER_ON } else { BORDER_COLOR };

    let clickable = on_press.is_some();
    let mut btn = button(text(label).size(10).color(text_color))
        .style(move |_: &Theme, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered if clickable => bg_hover,
                _ => bg_normal,
            })),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 2.0.into(),
            },
            text_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .padding([2, 6]);

    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }

    btn.into()
}

fn status_pill(label: impl Into<String>) -> Element<'static, Message> {
    container(text(label.into()).size(10).color(Color {
        r: 0.65,
        g: 0.65,
        b: 0.65,
        a: 1.0,
    }))
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(PILL_BG)),
        border: Border {
            color: BORDER_COLOR,
            width: 1.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    })
    .padding([2, 6])
    .into()
}

// ── Colours ───────────────────────────────────────────────────────────────

const BAR_BG: Color = Color {
    r: 0.14,
    g: 0.14,
    b: 0.14,
    a: 1.0,
};
const TAB_ACTIVE: Color = Color {
    r: 0.25,
    g: 0.25,
    b: 0.25,
    a: 1.0,
};
const TAB_HOVER: Color = Color {
    r: 0.20,
    g: 0.20,
    b: 0.20,
    a: 1.0,
};
const PILL_BG: Color = Color {
    r: 0.19,
    g: 0.19,
    b: 0.19,
    a: 1.0,
};
const BORDER_COLOR: Color = Color {
    r: 0.28,
    g: 0.28,
    b: 0.28,
    a: 1.0,
};
const ICON_COLOR: Color = Color {
    r: 0.70,
    g: 0.70,
    b: 0.70,
    a: 1.0,
};
const ACCENT: Color = Color {
    r: 0.20,
    g: 0.55,
    b: 0.90,
    a: 1.0,
};

const OSNAP_ON_TEXT: Color = Color {
    r: 0.35,
    g: 0.75,
    b: 1.00,
    a: 1.0,
};
const OSNAP_OFF_TEXT: Color = Color {
    r: 0.42,
    g: 0.42,
    b: 0.42,
    a: 1.0,
};
const SNAP_ON_BG: Color = Color {
    r: 0.10,
    g: 0.20,
    b: 0.32,
    a: 1.0,
};
const SNAP_ON_HOVER: Color = Color {
    r: 0.14,
    g: 0.27,
    b: 0.42,
    a: 1.0,
};
const SNAP_BORDER_ON: Color = Color {
    r: 0.20,
    g: 0.50,
    b: 0.85,
    a: 1.0,
};
const SNAP_OFF_BG: Color = Color {
    r: 0.17,
    g: 0.17,
    b: 0.17,
    a: 1.0,
};
const SNAP_OFF_HOVER: Color = Color {
    r: 0.22,
    g: 0.22,
    b: 0.22,
    a: 1.0,
};

// ── Scale popup button ────────────────────────────────────────────────────

fn scale_popup_btn(label: &str, open: bool) -> Element<'static, Message> {
    let label = label.to_string();
    button(
        text(label)
            .size(10)
            .color(if open { SNAP_BORDER_ON } else { OSNAP_OFF_TEXT }),
    )
    .on_press(Message::ToggleScalePopup)
    .style(move |_: &Theme, status| button::Style {
        background: Some(Background::Color(match (open, status) {
            (true, button::Status::Hovered) => SNAP_ON_HOVER,
            (true, _) => SNAP_ON_BG,
            (false, button::Status::Hovered) => SNAP_OFF_HOVER,
            (false, _) => SNAP_OFF_BG,
        })),
        border: Border {
            color: if open { SNAP_BORDER_ON } else { BORDER_COLOR },
            width: 1.0,
            radius: 2.0.into(),
        },
        text_color: if open { SNAP_BORDER_ON } else { OSNAP_OFF_TEXT },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .padding([2, 6])
    .into()
}

// ── Scale display ─────────────────────────────────────────────────────────

/// Formats a viewport scale factor as a human-readable ratio string.
///
/// - `None`  → "1:1"  (model space or no viewport yet)
/// - `1.0`   → "1:1"
/// - `0.02`  → "1:50"
/// - `2.0`   → "2:1"
fn format_scale(scale: Option<f64>) -> String {
    let s = match scale {
        None => return "1:1".to_string(),
        Some(v) if v <= 0.0 => return "1:1".to_string(),
        Some(v) => v,
    };

    // Try to express as a clean integer ratio.
    if s >= 1.0 {
        let n = s.round() as u32;
        if (s - n as f64).abs() < 0.01 * s {
            return if n == 1 {
                "1:1".to_string()
            } else {
                format!("{}:1", n)
            };
        }
    } else {
        let inv = (1.0 / s).round() as u32;
        if (s - 1.0 / inv as f64).abs() < 0.01 * s {
            return format!("1:{}", inv);
        }
    }

    // Fall back to a decimal string.
    format!("{:.4}", s)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
