use super::{document::HistorySnapshot, OpenCADStudio};
use std::collections::HashSet;

impl OpenCADStudio {
    pub(super) fn history_label_from_active_cmd(&self, i: usize, fallback: &'static str) -> String {
        self.tabs[i]
            .active_cmd
            .as_ref()
            .map(|cmd| cmd.name().to_string())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(super) fn capture_history_snapshot(
        &self,
        i: usize,
        label: impl Into<String>,
    ) -> HistorySnapshot {
        HistorySnapshot {
            document: self.tabs[i].scene.document.clone(),
            current_layout: self.tabs[i].scene.current_layout.clone(),
            selected: self.tabs[i].scene.selected.iter().copied().collect(),
            dirty: self.tabs[i].dirty,
            label: label.into(),
        }
    }

    pub(super) fn push_undo_snapshot(&mut self, i: usize, label: impl Into<String>) {
        let snapshot = self.capture_history_snapshot(i, label);
        self.tabs[i].history.undo_stack.push(snapshot);
        self.tabs[i].history.redo_stack.clear();
    }

    pub(super) fn restore_history_snapshot(&mut self, i: usize, snapshot: HistorySnapshot) {
        self.tabs[i].scene.document = snapshot.document;
        self.tabs[i].scene.set_current_layout(snapshot.current_layout);
        // Force a re-tessellation: the cached wires were keyed against the
        // outgoing document / layout and would be returned unchanged
        // otherwise (`set_current_layout` only bumps on actual change).
        self.tabs[i].scene.bump_geometry();
        self.tabs[i].scene.selected = snapshot
            .selected
            .into_iter()
            .filter(|h| self.tabs[i].scene.document.get_entity(*h).is_some())
            .collect::<HashSet<_>>();
        self.tabs[i].scene.populate_hatches_from_document();
        self.tabs[i].scene.populate_images_from_document();
        self.tabs[i].scene.populate_meshes_from_document();
        self.tabs[i].scene.clear_preview_wire();
        self.tabs[i].scene.images.clear();
        self.tabs[i].active_cmd = None;
        self.tabs[i].snap_result = None;
        self.tabs[i].active_grip = None;
        self.tabs[i].dirty = snapshot.dirty;
        let doc_layers = self.tabs[i].scene.document.layers.clone();
        let vp_info = self.tabs[i].scene.viewport_list();
        self.tabs[i]
            .layers
            .sync_with_viewports(&doc_layers, vp_info);
        self.sync_ribbon_layers();
        self.refresh_properties();
    }

    pub(super) fn undo_active_tab(&mut self) {
        self.undo_steps(1);
    }

    pub(super) fn redo_active_tab(&mut self) {
        self.redo_steps(1);
    }

    pub(super) fn undo_steps(&mut self, steps: usize) {
        let i = self.active_tab;
        let available = self.tabs[i].history.undo_stack.len();
        let steps = steps.min(available);
        if steps == 0 {
            self.command_line.push_info("Nothing to undo.");
            return;
        }

        let mut last_label = String::new();
        for _ in 0..steps {
            let Some(snapshot) = self.tabs[i].history.undo_stack.pop() else {
                break;
            };
            let label = snapshot.label.clone();
            let current = self.capture_history_snapshot(i, label.clone());
            self.tabs[i].history.redo_stack.push(current);
            self.restore_history_snapshot(i, snapshot);
            last_label = label;
        }
        self.command_line
            .push_output(&format!("Undo: {last_label}"));
    }

    pub(super) fn redo_steps(&mut self, steps: usize) {
        let i = self.active_tab;
        let available = self.tabs[i].history.redo_stack.len();
        let steps = steps.min(available);
        if steps == 0 {
            self.command_line.push_info("Nothing to redo.");
            return;
        }

        let mut last_label = String::new();
        for _ in 0..steps {
            let Some(snapshot) = self.tabs[i].history.redo_stack.pop() else {
                break;
            };
            let label = snapshot.label.clone();
            let current = self.capture_history_snapshot(i, label.clone());
            self.tabs[i].history.undo_stack.push(current);
            self.restore_history_snapshot(i, snapshot);
            last_label = label;
        }
        self.command_line
            .push_output(&format!("Redo: {last_label}"));
    }
}

pub(super) fn history_dropdown_labels(stack: &[HistorySnapshot]) -> Vec<String> {
    stack
        .iter()
        .rev()
        .map(|snapshot| snapshot.label.clone())
        .collect()
}
