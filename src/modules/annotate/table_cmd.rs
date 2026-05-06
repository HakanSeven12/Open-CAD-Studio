// TABLE command — create an empty table entity.
//
// Workflow:
//   1. Text: Enter number of columns  (default 3)
//   2. Text: Enter number of rows     (default 4, includes header row)
//   3. Point: Click insertion point
//
// Creates a Table entity with uniform row height (0.5) and column width (2.0).

use acadrust::entities::TableBuilder;
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::wire_model::WireModel;

pub const ICON: IconKind = IconKind::Svg(include_bytes!("../../../assets/icons/table.svg"));

pub fn tool() -> ToolDef {
    ToolDef {
        id: "TABLE",
        label: "Table",
        icon: ICON,
        event: ModuleEvent::Command("TABLE".to_string()),
    }
}

const DEFAULT_COLS: usize = 3;
const DEFAULT_ROWS: usize = 4;
const COL_WIDTH: f64 = 2.0;
const ROW_HEIGHT: f64 = 0.5;

enum Step {
    Columns,
    Rows { cols: usize },
    Insertion { cols: usize, rows: usize },
}

pub struct TableCommand {
    step: Step,
}

impl TableCommand {
    pub fn new() -> Self {
        Self { step: Step::Columns }
    }
}

impl CadCommand for TableCommand {
    fn name(&self) -> &'static str { "TABLE" }

    fn prompt(&self) -> String {
        match &self.step {
            Step::Columns => format!("TABLE  Enter number of columns [{DEFAULT_COLS}]:"),
            Step::Rows { cols } => format!("TABLE  Enter number of rows (incl. header) [{DEFAULT_ROWS}]  ({cols} cols):"),
            Step::Insertion { cols, rows } => format!("TABLE  Specify insertion point  [{cols}×{rows}]:"),
        }
    }

    fn wants_text_input(&self) -> bool {
        !matches!(self.step, Step::Insertion { .. })
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        let t = text.trim();
        match &self.step {
            Step::Columns => {
                let cols = if t.is_empty() {
                    DEFAULT_COLS
                } else {
                    t.parse::<usize>().ok().filter(|&n| n >= 1)?
                };
                self.step = Step::Rows { cols };
                Some(CmdResult::NeedPoint)
            }
            Step::Rows { cols } => {
                let cols = *cols;
                let rows = if t.is_empty() {
                    DEFAULT_ROWS
                } else {
                    t.parse::<usize>().ok().filter(|&n| n >= 1)?
                };
                self.step = Step::Insertion { cols, rows };
                Some(CmdResult::NeedPoint)
            }
            Step::Insertion { .. } => None,
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        match &self.step {
            Step::Columns => {
                // Accept default.
                self.step = Step::Rows { cols: DEFAULT_COLS };
                CmdResult::NeedPoint
            }
            Step::Rows { cols } => {
                let cols = *cols;
                self.step = Step::Insertion { cols, rows: DEFAULT_ROWS };
                CmdResult::NeedPoint
            }
            Step::Insertion { .. } => CmdResult::Cancel,
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Step::Insertion { cols, rows } = self.step {
            let ins = Vector3::new(pt.x as f64, pt.z as f64, pt.y as f64);
            let table = TableBuilder::new(rows, cols)
                .at(ins)
                .row_height(ROW_HEIGHT)
                .column_width(COL_WIDTH)
                .build();
            CmdResult::CommitAndExit(EntityType::Table(table))
        } else {
            CmdResult::NeedPoint
        }
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        if let Step::Insertion { cols, rows } = self.step {
            // Preview: outline of the table bounding box.
            let w = (cols as f32) * COL_WIDTH as f32;
            let h = (rows as f32) * ROW_HEIGHT as f32;
            let x = pt.x;
            let y = pt.y;
            let z = pt.z;
            Some(WireModel {
                name: "table_preview".into(),
                points: vec![
                    [x, y, z], [x + w, y, z],
                    [x + w, y, z], [x + w, y, z - h],
                    [x + w, y, z - h], [x, y, z - h],
                    [x, y, z - h], [x, y, z],
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
            })
        } else {
            None
        }
    }
}
