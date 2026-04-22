use crate::app::Message;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Background, Border, Color, Element, Fill, Theme};

const BG:     Color = Color { r: 0.15, g: 0.15, b: 0.15, a: 1.0 };
const PANEL:  Color = Color { r: 0.12, g: 0.12, b: 0.12, a: 1.0 };
const BORDER: Color = Color { r: 0.30, g: 0.30, b: 0.30, a: 1.0 };
const DIM:    Color = Color { r: 0.55, g: 0.55, b: 0.55, a: 1.0 };
const ACCENT: Color = Color { r: 0.25, g: 0.50, b: 0.85, a: 1.0 };
const WHITE:  Color = Color { r: 0.92, g: 0.92, b: 0.92, a: 1.0 };

fn info_row<'a>(label: &'static str, value: String) -> Element<'a, Message> {
    row![
        text(label).size(11).color(DIM).width(100),
        text(value).size(11).color(WHITE),
    ]
    .spacing(8)
    .align_y(iced::Center)
    .padding([3, 0])
    .into()
}

pub fn view_window<'a>() -> Element<'a, Message> {
    let version   = env!("CARGO_PKG_VERSION");
    let os        = std::env::consts::OS;
    let arch      = std::env::consts::ARCH;

    let logo = container(
        column![
            text("H7CAD").size(32).color(ACCENT),
            text("CAD application for Architecture & Engineering")
                .size(11).color(DIM),
        ]
        .spacing(4)
        .align_x(iced::Center),
    )
    .width(Fill)
    .padding(iced::Padding { top: 20.0, right: 0.0, bottom: 16.0, left: 0.0 })
    .align_x(iced::Center);

    let info_block = container(
        column![
            info_row("Version",   format!("v{}", version)),
            info_row("Platform",  os.to_string()),
            info_row("Arch",      arch.to_string()),
        ]
        .spacing(2)
        .padding([12, 16]),
    )
    .width(Fill)
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    });

    let copy_btn = button(
        text("Copy Info").size(11),
    )
    .on_press(Message::AboutCopyInfo)
    .style(|_: &Theme, st| button::Style {
        background: Some(Background::Color(match st {
            button::Status::Hovered | button::Status::Pressed =>
                Color { r: 0.20, g: 0.42, b: 0.72, a: 1.0 },
            _ => ACCENT,
        })),
        text_color: WHITE,
        border: Border { radius: 4.0.into(), ..Default::default() },
        ..Default::default()
    })
    .padding([6, 16]);

    let footer = row![
        Space::new().width(Fill),
        copy_btn,
    ]
    .align_y(iced::Center)
    .padding(iced::Padding { top: 12.0, right: 0.0, bottom: 0.0, left: 0.0 });

    container(
        column![logo, info_block, footer]
            .spacing(0)
            .padding(iced::Padding { top: 0.0, right: 20.0, bottom: 20.0, left: 20.0 }),
    )
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(BG)),
        ..Default::default()
    })
    .width(Fill)
    .height(Fill)
    .into()
}
