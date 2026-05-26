//! OpenCADStudio-style command line — bottom panel with input and history

use crate::app::Message;
use iced::widget::{column, container, row, text, text_input};
use iced::{Background, Border, Color, Element, Length, Theme};

pub const CMD_INPUT_ID: &str = "cmd_input";

fn cmd_input_id() -> iced::widget::Id {
    iced::widget::Id::new(CMD_INPUT_ID)
}

const MAX_HISTORY: usize = 64;

#[derive(Clone, Default)]
pub struct CommandLine {
    pub input: String,
    pub history: Vec<HistoryEntry>,
    /// Commands the user has typed (for ↑/↓ recall).
    pub cmd_recall: Vec<String>,
    /// Current position in `cmd_recall` while navigating (None = not navigating).
    recall_cursor: Option<usize>,
    /// Saved draft input before the user started navigating history.
    recall_draft: String,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub kind: EntryKind,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum EntryKind {
    Command,
    Output,
    Error,
    Info,
}

impl CommandLine {
    pub fn new() -> Self {
        let mut cl = Self::default();
        cl.push_info("Open CAD Studio ready.");
        cl.push_info("Type a command or use the ribbon. Open OBJ: INSERT tab.");
        cl
    }

    pub fn submit(&mut self) -> Option<String> {
        let cmd = self.input.trim().to_uppercase();
        if cmd.is_empty() {
            return None;
        }
        // Record in recall list (avoid duplicates at the top).
        let raw = self.input.trim().to_string();
        if self.cmd_recall.last().map(|s| s.as_str()) != Some(&raw) {
            self.cmd_recall.push(raw);
            if self.cmd_recall.len() > 50 {
                self.cmd_recall.remove(0);
            }
        }
        self.recall_cursor = None;
        self.recall_draft.clear();
        self.push_command(&self.input.clone());
        self.input.clear();
        Some(cmd)
    }

    /// Navigate to the previous command in recall history (↑).
    pub fn history_prev(&mut self) {
        if self.cmd_recall.is_empty() {
            return;
        }
        let cursor = match self.recall_cursor {
            None => {
                self.recall_draft = self.input.clone();
                self.cmd_recall.len() - 1
            }
            Some(c) if c > 0 => c - 1,
            Some(c) => c,
        };
        self.recall_cursor = Some(cursor);
        self.input = self.cmd_recall[cursor].clone();
    }

    /// Navigate to the next command in recall history (↓).
    pub fn history_next(&mut self) {
        match self.recall_cursor {
            None => {}
            Some(c) if c + 1 < self.cmd_recall.len() => {
                let next = c + 1;
                self.recall_cursor = Some(next);
                self.input = self.cmd_recall[next].clone();
            }
            Some(_) => {
                self.recall_cursor = None;
                self.input = self.recall_draft.clone();
            }
        }
    }

    pub fn push_command(&mut self, cmd: &str) {
        self.push(EntryKind::Command, format!("Command: {cmd}"));
    }
    pub fn push_output(&mut self, msg: &str) {
        self.push(EntryKind::Output, msg.to_string());
    }
    pub fn push_error(&mut self, msg: &str) {
        self.push(EntryKind::Error, format!("*Invalid*  {msg}"));
    }
    pub fn push_info(&mut self, msg: &str) {
        self.push(EntryKind::Info, msg.to_string());
    }
    fn push(&mut self, kind: EntryKind, text: String) {
        self.history.push(HistoryEntry { kind, text });
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let history_rows =
            self.history
                .iter()
                .rev()
                .take(4)
                .rev()
                .fold(column![].spacing(0), |col, entry| {
                    let color = match entry.kind {
                        EntryKind::Command => CMD_COLOR,
                        EntryKind::Output => OUT_COLOR,
                        EntryKind::Error => ERR_COLOR,
                        EntryKind::Info => INFO_COLOR,
                    };
                    col.push(container(text(&entry.text).size(11).color(color)).padding([1, 8]))
                });
        let prompt = container(text("Command:").size(11).color(PROMPT_COLOR)).padding([5, 8]);
        let input = text_input("", &self.input)
            .id(cmd_input_id())
            .on_input(Message::CommandInput)
            .on_submit(Message::CommandSubmit)
            .style(|_: &Theme, _| text_input::Style {
                background: Background::Color(INPUT_BG),
                border: Border {
                    color: Color {
                        r: 0.40,
                        g: 0.60,
                        b: 0.90,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                icon: Color::WHITE,
                placeholder: Color {
                    r: 0.4,
                    g: 0.4,
                    b: 0.4,
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
            .size(11)
            .padding([4, 6]);
        let input_row = row![prompt, input].align_y(iced::Center);
        container(column![
            container(history_rows)
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(HISTORY_BG)),
                    ..Default::default()
                })
                .width(Length::Fill)
                .padding([2, 0]),
            container(input_row)
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(INPUT_ROW_BG)),
                    border: Border {
                        color: BORDER_COLOR,
                        width: 1.0,
                        radius: 3.0.into()
                    },
                    ..Default::default()
                })
                .width(Length::Fill),
        ])
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(PANEL_BG)),
            border: Border {
                color: BORDER_COLOR,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .width(Length::Fixed(720.0))
        .into()
    }
}

const PANEL_BG: Color = Color {
    r: 0.15,
    g: 0.15,
    b: 0.15,
    a: 1.0,
};
const HISTORY_BG: Color = Color {
    r: 0.15,
    g: 0.15,
    b: 0.15,
    a: 1.0,
};
const INPUT_ROW_BG: Color = Color {
    r: 0.18,
    g: 0.18,
    b: 0.18,
    a: 1.0,
};
const INPUT_BG: Color = Color {
    r: 0.12,
    g: 0.12,
    b: 0.12,
    a: 1.0,
};
const BORDER_COLOR: Color = Color {
    r: 0.30,
    g: 0.30,
    b: 0.30,
    a: 1.0,
};
const PROMPT_COLOR: Color = Color {
    r: 0.55,
    g: 0.78,
    b: 0.55,
    a: 1.0,
};
const CMD_COLOR: Color = Color {
    r: 0.80,
    g: 0.80,
    b: 0.80,
    a: 1.0,
};
const OUT_COLOR: Color = Color {
    r: 0.65,
    g: 0.65,
    b: 0.65,
    a: 1.0,
};
const ERR_COLOR: Color = Color {
    r: 0.90,
    g: 0.35,
    b: 0.35,
    a: 1.0,
};
const INFO_COLOR: Color = Color {
    r: 0.50,
    g: 0.70,
    b: 0.90,
    a: 1.0,
};
