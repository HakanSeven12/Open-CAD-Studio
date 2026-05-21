use crate::modules::{IconKind, ModuleEvent, ToolDef};

#[allow(dead_code)]
pub fn tool() -> ToolDef {
    ToolDef {
        id: "CHANGELOG",
        label: "Changelog",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/changelog.svg")),
        event: ModuleEvent::Command("CHANGELOG".to_string()),
    }
}
