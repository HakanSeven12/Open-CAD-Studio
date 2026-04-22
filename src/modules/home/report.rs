use crate::modules::{IconKind, ModuleEvent, ToolDef};

pub fn tool() -> ToolDef {
    ToolDef {
        id: "REPORT",
        label: "Report",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/report.svg")),
        event: ModuleEvent::Command("REPORT".to_string()),
    }
}
