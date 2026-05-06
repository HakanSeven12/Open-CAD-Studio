// ATTDEF command — define a block attribute (AttributeDefinition entity).
//
// Workflow (command-line only):
//   1. Text: Enter attribute tag    (required, no spaces)
//   2. Text: Enter attribute prompt (optional — press Enter to use tag)
//   3. Text: Enter default value    (optional — press Enter for blank)
//   4. Point: Click insertion point

use acadrust::entities::AttributeDefinition;
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;

enum Step {
    Tag,
    Prompt { tag: String },
    Default { tag: String, prompt: String },
    Insertion { tag: String, prompt: String, default: String },
}

pub struct AttdefCommand {
    step: Step,
    /// Text height in world units.
    height: f64,
}

impl AttdefCommand {
    pub fn new() -> Self {
        Self { step: Step::Tag, height: 0.2 }
    }
}

impl CadCommand for AttdefCommand {
    fn name(&self) -> &'static str { "ATTDEF" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::Tag         => "ATTDEF  Enter attribute tag (no spaces):".into(),
            Step::Prompt { tag } => format!("ATTDEF  Enter prompt for '{tag}' (Enter=use tag):"),
            Step::Default { tag, .. } => format!("ATTDEF  Enter default value for '{tag}' (Enter=blank):"),
            Step::Insertion { tag, .. } => format!("ATTDEF  Specify insertion point for '{tag}':"),
        }
    }

    fn wants_text_input(&self) -> bool {
        !matches!(self.step, Step::Insertion { .. })
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        match &self.step {
            Step::Tag => {
                let tag = text.trim().replace(' ', "_");
                if tag.is_empty() {
                    return Some(CmdResult::NeedPoint);
                }
                self.step = Step::Prompt { tag };
                Some(CmdResult::NeedPoint)
            }
            Step::Prompt { tag } => {
                let tag = tag.clone();
                let prompt = if text.trim().is_empty() { tag.clone() } else { text.trim().to_string() };
                self.step = Step::Default { tag, prompt };
                Some(CmdResult::NeedPoint)
            }
            Step::Default { tag, prompt } => {
                let tag = tag.clone();
                let prompt = prompt.clone();
                let default = text.trim().to_string();
                self.step = Step::Insertion { tag, prompt, default };
                Some(CmdResult::NeedPoint)
            }
            Step::Insertion { .. } => None,
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Step::Insertion { tag, prompt, default } = &self.step {
            let mut attdef = AttributeDefinition {
                tag: tag.clone(),
                prompt: prompt.clone(),
                default_value: default.clone(),
                insertion_point: Vector3::new(pt.x as f64, pt.z as f64, pt.y as f64),
                height: self.height,
                ..Default::default()
            };
            attdef.common.layer = "0".into();
            CmdResult::CommitAndExit(EntityType::AttributeDefinition(attdef))
        } else {
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        match &self.step {
            Step::Tag => CmdResult::Cancel,
            // Treat Enter as empty text input for prompt/default steps.
            Step::Prompt { tag } => {
                let tag = tag.clone();
                self.step = Step::Default { tag: tag.clone(), prompt: tag };
                CmdResult::NeedPoint
            }
            Step::Default { tag, prompt } => {
                let (tag, prompt) = (tag.clone(), prompt.clone());
                self.step = Step::Insertion { tag, prompt, default: String::new() };
                CmdResult::NeedPoint
            }
            Step::Insertion { .. } => CmdResult::Cancel,
        }
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if !matches!(self.step, Step::Insertion { .. }) {
            return None;
        }
        // Show a small cross at the insertion point.
        let d = 0.15_f32;
        Some(WireModel {
            name: "attdef_preview".into(),
            points: vec![
                [pt.x - d, pt.y, pt.z], [pt.x + d, pt.y, pt.z],
                [f32::NAN, 0.0, 0.0],
                [pt.x, pt.y, pt.z - d], [pt.x, pt.y, pt.z + d],
            ],
            color: WireModel::CYAN,
            selected: false,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px: 1.0,
            snap_pts: vec![],
            tangent_geoms: vec![],
            aci: 0,
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: vec![],
        })
    }
}
