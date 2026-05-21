use crate::modules::{IconKind, ModuleEvent, ToolDef};

// Kept for the command-line route ("DONATE" cmd) and possible future ribbon
// re-introduction; currently the Start tab invokes the same command directly.
#[allow(dead_code)]
pub fn tool() -> ToolDef {
    ToolDef {
        id: "DONATE",
        label: "Donate",
        icon: IconKind::Svg(include_bytes!("../../../assets/icons/donate.svg")),
        event: ModuleEvent::Command("DONATE".to_string()),
    }
}
