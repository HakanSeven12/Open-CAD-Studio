use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "ABOUT",
        label: "About",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/about.svg")),
        event: ModuleEvent::Command("ABOUT".to_string()),
    }
}
