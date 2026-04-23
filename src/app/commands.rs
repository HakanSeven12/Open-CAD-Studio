use super::{H7CAD, Message};
use crate::command::CadCommand;
use crate::scene::Scene;
use iced::Task;
use std::path::PathBuf;

impl H7CAD {
    pub(super) fn dispatch_command(&mut self, cmd: &str) -> Task<Message> {
        let i = self.active_tab;
        // Cancel any running command before starting a new one.
        if self.tabs[i].active_cmd.is_some() {
            self.tabs[i].scene.clear_preview_wire();
            self.tabs[i].active_cmd = None;
        }
        // Reset the last committed point so the first click of the new command
        // is not constrained by ortho/polar relative to a previous command's endpoint.
        self.last_point = None;

        if let Some(path_str) = cmd.strip_prefix("OPEN_RECENT:") {
            let path = PathBuf::from(path_str);
            return Task::perform(crate::io::open_path(path), Message::FileOpened);
        }

        match cmd {
            "NEW"                => return Task::done(Message::ClearScene),
            "OPEN"               => return Task::done(Message::OpenFile),
            "SAVE"|"QSAVE"       => return Task::done(Message::SaveFile),
            "SAVEAS"             => return Task::done(Message::SaveAs),
            "UNDO"|"U"           => return Task::done(Message::Undo),
            "REDO"               => return Task::done(Message::Redo),
            "CLEAR"|"CLR"        => return Task::done(Message::ClearScene),
            "WIREFRAME"|"VW"     => return Task::done(Message::SetWireframe(true)),
            "SOLID"|"VS"         => return Task::done(Message::SetWireframe(false)),

            // ── Background color ───────────────────────────────────────────
            // Usage:  BACKGROUND <r> <g> <b>   (0–255 each)
            //         BACKGROUND RESET          (restore default)
            cmd if cmd == "BACKGROUND" || cmd.starts_with("BACKGROUND ") => {
                let args = cmd.split_whitespace().skip(1).collect::<Vec<_>>();
                let is_paper = self.tabs[i].scene.current_layout != "Model";
                if args.first().map(|s| s.eq_ignore_ascii_case("RESET")).unwrap_or(false) {
                    if is_paper {
                        self.tabs[i].paper_bg_color = None;
                        self.tabs[i].scene.paper_bg_color = [1.0, 1.0, 1.0, 1.0];
                    } else {
                        self.tabs[i].bg_color = None;
                        self.tabs[i].scene.bg_color = [0.11, 0.11, 0.11, 1.0];
                    }
                    self.command_line.push_output("Background reset to default.");
                } else if args.len() >= 3 {
                    let r = args[0].parse::<u8>().unwrap_or(0) as f32 / 255.0;
                    let g = args[1].parse::<u8>().unwrap_or(0) as f32 / 255.0;
                    let b = args[2].parse::<u8>().unwrap_or(0) as f32 / 255.0;
                    if is_paper {
                        self.tabs[i].paper_bg_color = Some([r, g, b, 1.0]);
                        self.tabs[i].scene.paper_bg_color = [r, g, b, 1.0];
                    } else {
                        self.tabs[i].bg_color = Some([r, g, b, 1.0]);
                        self.tabs[i].scene.bg_color = [r, g, b, 1.0];
                    }
                    self.command_line
                        .push_output(&format!("Background: rgb({}, {}, {})", args[0], args[1], args[2]));
                } else {
                    self.command_line.push_info(
                        "Usage: BACKGROUND <r> <g> <b>  (0–255)  |  BACKGROUND RESET"
                    );
                }
            }
            "ORTHO"              => return Task::done(Message::SetProjection(true)),
            "PERSP"              => return Task::done(Message::SetProjection(false)),
            "LAYERS"|"LA"        => return Task::done(Message::ToggleLayers),

            // ── Layer object commands ──────────────────────────────────────
            "LAYOFF" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("LAYOFF");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let layers: std::collections::HashSet<String> = self.tabs[i].scene
                        .selected_entities().into_iter()
                        .map(|(_, e)| e.common().layer.clone()).collect();
                    self.push_undo_snapshot(i, "LAYOFF");
                    for name in &layers {
                        if name == "0" { continue; }
                        if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(name) {
                            dl.turn_off();
                        }
                    }
                    self.tabs[i].dirty = true;
                    self.sync_ribbon_layers();
                    self.command_line.push_info("Layer(s) turned off.");
                }
            }

            "LAYFRZ" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("LAYFRZ");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let layers: std::collections::HashSet<String> = self.tabs[i].scene
                        .selected_entities().into_iter()
                        .map(|(_, e)| e.common().layer.clone()).collect();
                    self.push_undo_snapshot(i, "LAYFRZ");
                    for name in &layers {
                        if name == "0" { continue; }
                        if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(name) {
                            dl.freeze();
                        }
                    }
                    self.tabs[i].dirty = true;
                    self.sync_ribbon_layers();
                    self.command_line.push_info("Layer(s) frozen.");
                }
            }

            "LAYLCK" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("LAYLCK");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let layers: std::collections::HashSet<String> = self.tabs[i].scene
                        .selected_entities().into_iter()
                        .map(|(_, e)| e.common().layer.clone()).collect();
                    self.push_undo_snapshot(i, "LAYLCK");
                    for name in &layers {
                        if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(name) {
                            dl.lock();
                        }
                    }
                    self.tabs[i].dirty = true;
                    self.sync_ribbon_layers();
                    self.command_line.push_info("Layer(s) locked.");
                }
            }

            "LAYMCUR" => {
                let entities = self.tabs[i].scene.selected_entities();
                if entities.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("LAYMCUR");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let layer = entities[0].1.common().layer.clone();
                    self.tabs[i].active_layer = layer.clone();
                    self.ribbon.active_layer = layer.clone();
                    self.tabs[i].layers.current_layer = layer.clone();
                    self.command_line.push_info(&format!("Current layer set to \"{layer}\"."));
                    self.sync_ribbon_layers();
                }
            }

            "LAYON" => {
                self.push_undo_snapshot(i, "LAYON");
                for name in self.tabs[i].scene.document.layers.iter()
                    .map(|l| l.name.clone()).collect::<Vec<_>>()
                {
                    if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(&name) {
                        dl.turn_on();
                    }
                }
                self.tabs[i].dirty = true;
                self.sync_ribbon_layers();
                self.command_line.push_info("All layers turned on.");
            }

            "LAYTHW" => {
                self.push_undo_snapshot(i, "LAYTHW");
                for name in self.tabs[i].scene.document.layers.iter()
                    .map(|l| l.name.clone()).collect::<Vec<_>>()
                {
                    if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(&name) {
                        dl.thaw();
                    }
                }
                self.tabs[i].dirty = true;
                self.sync_ribbon_layers();
                self.command_line.push_info("All layers thawed.");
            }

            "LAYULK" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("LAYULK");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let layers: std::collections::HashSet<String> = self.tabs[i].scene
                        .selected_entities().into_iter()
                        .map(|(_, e)| e.common().layer.clone()).collect();
                    self.push_undo_snapshot(i, "LAYULK");
                    for name in &layers {
                        if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(name) {
                            dl.unlock();
                        }
                    }
                    self.tabs[i].dirty = true;
                    self.sync_ribbon_layers();
                    self.command_line.push_info("Layer(s) unlocked.");
                }
            }

            // LAYISO — turn off all layers except those used by selected entities
            "LAYISO" => {
                let sel_layers: std::collections::HashSet<String> = self.tabs[i].scene
                    .selected_entities().into_iter()
                    .map(|(_, e)| e.common().layer.clone()).collect();
                if sel_layers.is_empty() {
                    self.command_line.push_error("LAYISO: select entities on the layers to isolate first.");
                } else {
                    self.push_undo_snapshot(i, "LAYISO");
                    let names: Vec<String> = self.tabs[i].scene.document.layers
                        .iter().map(|l| l.name.clone()).collect();
                    for name in names {
                        if !sel_layers.contains(&name) {
                            if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(&name) {
                                dl.turn_off();
                            }
                        }
                    }
                    self.tabs[i].dirty = true;
                    self.sync_ribbon_layers();
                    self.command_line.push_info(&format!(
                        "LAYISO: isolated {} layer(s).", sel_layers.len()
                    ));
                }
            }

            // LAYUNISO — restore all layers that were turned off by LAYISO (turn all on)
            "LAYUNISO" => {
                self.push_undo_snapshot(i, "LAYUNISO");
                let names: Vec<String> = self.tabs[i].scene.document.layers
                    .iter().map(|l| l.name.clone()).collect();
                for name in names {
                    if let Some(dl) = self.tabs[i].scene.document.layers.get_mut(&name) {
                        dl.turn_on();
                    }
                }
                self.tabs[i].dirty = true;
                self.sync_ribbon_layers();
                self.command_line.push_info("LAYUNISO: all layers restored.");
            }

            "LAYMATCH"|"LAYMCH" => {
                use crate::modules::home::layers::match_layer::LayMatchCommand;
                let dest: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                let cmd = LayMatchCommand::new(dest);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "MATCHPROP"|"MA" => {
                use crate::modules::home::properties::match_prop::MatchPropCommand;
                self.tabs[i].scene.deselect_all();
                let cmd = MatchPropCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "GROUP"|"G" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene.selected_entities().into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("GROUP");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let auto_name = super::helpers::next_group_auto_name(&self.tabs[i].scene);
                    use crate::modules::home::groups::group::GroupCommand;
                    let cmd = GroupCommand::new(handles, auto_name);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "UNGROUP"|"UG" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene.selected_entities().into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::groups::ungroup::UngroupCommand;
                    let cmd = UngroupCommand::new();
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    self.push_undo_snapshot(i, "UNGROUP");
                    let count = self.tabs[i].scene.delete_groups_containing(&handles);
                    self.tabs[i].dirty = true;
                    if count > 0 {
                        self.command_line.push_info(&format!("{} group(s) dissolved.", count));
                    } else {
                        self.command_line.push_info("No groups found for selected objects.");
                    }
                }
            }

            "COPYCLIP"|"CC" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene.selected_entities().into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("COPYCLIP");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let entities: Vec<_> = handles.iter()
                        .filter_map(|&h| self.tabs[i].scene.document.get_entity(h).cloned())
                        .collect();
                    self.clipboard_centroid = super::helpers::entities_centroid(
                        &self.tabs[i].scene.wire_models_for(&handles),
                    );
                    self.clipboard = entities;
                    self.command_line.push_info(
                        &format!("{} object(s) copied to clipboard.", self.clipboard.len()),
                    );
                }
            }

            "CUTCLIP"|"CX" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene.selected_entities().into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("CUTCLIP");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let entities: Vec<_> = handles.iter()
                        .filter_map(|&h| self.tabs[i].scene.document.get_entity(h).cloned())
                        .collect();
                    self.clipboard_centroid = super::helpers::entities_centroid(
                        &self.tabs[i].scene.wire_models_for(&handles),
                    );
                    let count = entities.len();
                    self.clipboard = entities;
                    self.push_undo_snapshot(i, "CUTCLIP");
                    self.tabs[i].scene.erase_entities(&handles);
                    self.tabs[i].scene.deselect_all();
                    self.tabs[i].dirty = true;
                    self.refresh_properties();
                    self.command_line.push_info(
                        &format!("{} object(s) cut to clipboard.", count),
                    );
                }
            }

            "PASTECLIP"|"PC" => {
                if self.clipboard.is_empty() {
                    self.command_line.push_error("Clipboard is empty.");
                } else {
                    let wires = self.tabs[i].scene.wires_for_entities(&self.clipboard);
                    use crate::modules::home::clipboard::paste::PasteCommand;
                    let cmd = PasteCommand::new(wires, self.clipboard_centroid);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            // PASTEORIG — paste at original coordinates (no move to pick point)
            "PASTEORIG" => {
                if self.clipboard.is_empty() {
                    self.command_line.push_error("PASTEORIG: clipboard is empty.");
                } else {
                    let count = self.clipboard.len();
                    self.push_undo_snapshot(i, "PASTEORIG");
                    for entity in &self.clipboard {
                        self.tabs[i].scene.add_entity(entity.clone());
                    }
                    self.tabs[i].dirty = true;
                    self.command_line.push_output(&format!(
                        "PASTEORIG: {} object(s) pasted at original coordinates.", count
                    ));
                }
            }

            "BLOCK" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene.selected_entities().into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("BLOCK");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::insert::create_block::CreateBlockCommand;
                    let cmd = CreateBlockCommand::new(handles);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "INSERT" => {
                let blocks = self.tabs[i].scene.custom_block_names();
                if blocks.is_empty() {
                    self.command_line
                        .push_error("No user-defined blocks found in this drawing.");
                } else {
                    use crate::modules::insert::insert_block::InsertBlockCommand;
                    let cmd = InsertBlockCommand::new(blocks);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "XATTACH" | "XA" => {
                // Launch the file picker; XAttachPickResult will start the command.
                return Task::done(Message::XAttachPick);
            }

            cmd if cmd == "WBLOCK" || cmd == "WB" || cmd.starts_with("WBLOCK ") => {
                let arg = cmd.splitn(2, ' ').nth(1).unwrap_or("").trim();
                if arg.is_empty() {
                    // No argument: use selected entities (*) if any, else ask.
                    let sel: Vec<_> = self.tabs[i].scene.selected.iter().copied().collect();
                    if sel.is_empty() {
                        self.command_line.push_error(
                            "WBLOCK  Select entities first, or: WBLOCK <block name>  or  WBLOCK *",
                        );
                    } else {
                        return Task::done(Message::WblockSave("*".to_string()));
                    }
                } else {
                    return Task::done(Message::WblockSave(arg.to_string()));
                }
            }

            "XREF" | "XR" => {
                // List all xref blocks in the current drawing.
                let xrefs: Vec<String> = self.tabs[i]
                    .scene
                    .document
                    .block_records
                    .iter()
                    .filter(|br| br.flags.is_xref || br.flags.is_xref_overlay)
                    .map(|br| {
                        format!(
                            "  {} — {}",
                            br.name,
                            if br.xref_path.is_empty() {
                                "(no path)".to_string()
                            } else {
                                br.xref_path.clone()
                            }
                        )
                    })
                    .collect();
                if xrefs.is_empty() {
                    self.command_line.push_output("XREF  No external references in this drawing.");
                } else {
                    self.command_line.push_output("XREF  External references:");
                    for line in xrefs {
                        self.command_line.push_output(&line);
                    }
                }
            }

            "XRELOAD" => {
                // Reload all xrefs for the current drawing.
                if let Some(path) = &self.tabs[i].current_path.clone() {
                    if let Some(base_dir) = path.parent() {
                        let infos = crate::io::xref::resolve_xrefs(
                            &mut self.tabs[i].scene.document,
                            base_dir,
                        );
                        for info in &infos {
                            match info.status {
                                crate::io::xref::XrefStatus::Loaded => {
                                    self.command_line.push_output(&format!(
                                        "XREF  Reloaded \"{}\"",
                                        info.name
                                    ));
                                }
                                crate::io::xref::XrefStatus::NotFound => {
                                    self.command_line.push_error(&format!(
                                        "XREF  Not found: \"{}\" ({})",
                                        info.name, info.path
                                    ));
                                }
                            }
                        }
                        self.tabs[i].scene.populate_hatches_from_document();
                        self.tabs[i].scene.populate_images_from_document();
                        self.tabs[i].scene.populate_meshes_from_document();
                    }
                } else {
                    self.command_line.push_error("XREF  Save the drawing first to resolve relative XREF paths.");
                }
            }

            // ── Draw commands ──────────────────────────────────────────────
            "LINE"|"L" => {
                use crate::modules::home::draw::line::LineCommand;
                let new_cmd = LineCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "MLINE"|"ML" => {
                use crate::modules::home::draw::mline::MlineCommand;
                let style = self.tabs[i].scene.document.header.multiline_style.clone();
                let cmd_obj = MlineCommand::with_style(style);
                self.command_line.push_info(&cmd_obj.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
            }

            cmd if cmd == "WIPEOUT" || cmd == "WO" || cmd.starts_with("WIPEOUT ") => {
                use crate::modules::home::draw::wipeout::WipeoutCommand;
                let args = cmd.split_once(' ').map(|(_, r)| r.trim().to_uppercase()).unwrap_or_default();
                let wo_cmd = if args == "P" || args == "POLYGONAL" {
                    WipeoutCommand::new_polygonal()
                } else {
                    WipeoutCommand::new_rectangular()
                };
                self.command_line.push_info(&wo_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(wo_cmd));
            }

            cmd if cmd == "IMAGE" || cmd == "IMAGEATTACH" || cmd == "IM" => {
                return Task::done(Message::ImagePick);
            }

            "REVCLOUD" => {
                use crate::modules::home::draw::revcloud::RevCloudCommand;
                let cmd = RevCloudCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "ATTDEF" => {
                use crate::modules::home::draw::attdef::AttdefCommand;
                let cmd = AttdefCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ATTEDIT — list or edit attribute values on selected Insert entities.
            // Usage:
            //   ATTEDIT           — list all attributes on selected Insert(s)
            //   ATTEDIT <tag> <v> — set the value of attribute <tag> to <v>
            cmd if cmd == "ATTEDIT" || cmd.starts_with("ATTEDIT ") => {
                let rest = cmd.trim_start_matches("ATTEDIT").trim();
                let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
                let selected_handles: Vec<acadrust::Handle> = self.tabs[i].scene
                    .selected_entities()
                    .iter()
                    .map(|(h, _)| *h)
                    .collect();
                if selected_handles.is_empty() {
                    self.command_line.push_error("ATTEDIT: select an Insert entity first.");
                } else {
                    let mut found_any = false;
                    for sh in &selected_handles {
                        if let Some(acadrust::EntityType::Insert(ins)) =
                            self.tabs[i].scene.document.entities().find(|e| e.common().handle == *sh)
                        {
                            found_any = true;
                            if rest.is_empty() {
                                // List attributes.
                                if ins.attributes.is_empty() {
                                    self.command_line.push_output(&format!(
                                        "  Insert {:x}: no attributes.", sh.value()
                                    ));
                                } else {
                                    for attr in &ins.attributes {
                                        self.command_line.push_output(&format!(
                                            "  [{tag}] = {val}",
                                            tag = attr.tag,
                                            val = attr.get_value()
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    if !found_any {
                        self.command_line.push_error("ATTEDIT: no Insert entities in selection.");
                    }
                    // If tag + value supplied, mutate attributes.
                    if parts.len() == 2 && !parts[0].is_empty() {
                        let tag_up = parts[0].to_uppercase();
                        let new_val = parts[1];
                        let mut changed = 0usize;
                        self.push_undo_snapshot(i, "ATTEDIT");
                        for sh in &selected_handles {
                            if let Some(acadrust::EntityType::Insert(ins)) =
                                self.tabs[i].scene.document.entities_mut().find(|e| e.common().handle == *sh)
                            {
                                for attr in &mut ins.attributes {
                                    if attr.tag.to_uppercase() == tag_up {
                                        attr.set_value(new_val);
                                        changed += 1;
                                    }
                                }
                            }
                        }
                        if changed > 0 {
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!(
                                "ATTEDIT: updated {changed} attribute(s) [{tag_up}] = {new_val}."
                            ));
                        } else {
                            self.command_line.push_error(&format!(
                                "ATTEDIT: tag '{tag_up}' not found in selection."
                            ));
                        }
                    }
                }
            }

            // ATTDISP — control attribute display visibility.
            // ATTDISP ON   — make all AttributeDefinitions visible
            // ATTDISP OFF  — make all AttributeDefinitions invisible
            // ATTDISP NORMAL — restore: show only those without the invisible flag
            cmd if cmd == "ATTDISP" || cmd.starts_with("ATTDISP ") => {
                let sub = cmd.split_whitespace().nth(1).unwrap_or("").to_uppercase();
                match sub.as_str() {
                    "ON" | "OFF" | "NORMAL" => {
                        self.push_undo_snapshot(i, "ATTDISP");
                        let mut count = 0usize;
                        for entity in self.tabs[i].scene.document.entities_mut() {
                            if let acadrust::EntityType::AttributeDefinition(ad) = entity {
                                match sub.as_str() {
                                    "ON"     => { ad.flags.invisible = false; count += 1; }
                                    "OFF"    => { ad.flags.invisible = true;  count += 1; }
                                    "NORMAL" => { /* leave existing flags — they are already the "normal" state */ }
                                    _ => {}
                                }
                            }
                        }
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!(
                            "ATTDISP {sub}: {count} attribute definition(s) updated."
                        ));
                    }
                    _ => {
                        self.command_line.push_info("Usage: ATTDISP ON | OFF | NORMAL");
                    }
                }
            }

            "DONUT"|"DO" => {
                use crate::modules::home::draw::donut::DonutCommand;
                let cmd = DonutCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "CIRCLE"|"C" => {
                use crate::modules::home::draw::circle::CircleCommand;
                let new_cmd = CircleCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "CIRCLE_CD" => {
                use crate::modules::home::draw::circle::CircleCDCommand;
                let new_cmd = CircleCDCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "CIRCLE_2P" => {
                use crate::modules::home::draw::circle::Circle2PCommand;
                let new_cmd = Circle2PCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "CIRCLE_3P" => {
                use crate::modules::home::draw::circle::Circle3PCommand;
                let new_cmd = Circle3PCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "CIRCLE_TTR" => {
                use crate::modules::home::draw::circle::CircleTTRCommand;
                let new_cmd = CircleTTRCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.pre_cmd_tangent = Some(self.snapper.is_on(crate::snap::SnapType::Tangent));
                self.snapper.enabled.insert(crate::snap::SnapType::Tangent);
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "CIRCLE_TTT" => {
                use crate::modules::home::draw::circle::CircleTTTCommand;
                let new_cmd = CircleTTTCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.pre_cmd_tangent = Some(self.snapper.is_on(crate::snap::SnapType::Tangent));
                self.snapper.enabled.insert(crate::snap::SnapType::Tangent);
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "ARC"|"A" => {
                use crate::modules::home::draw::arc::ArcCommand;
                let new_cmd = ArcCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_3P" => {
                use crate::modules::home::draw::arc::Arc3PCommand;
                let new_cmd = Arc3PCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_SCE" => {
                use crate::modules::home::draw::arc::ArcSCECommand;
                let new_cmd = ArcSCECommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_SCA" => {
                use crate::modules::home::draw::arc::ArcSCACommand;
                let new_cmd = ArcSCACommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_SCL" => {
                use crate::modules::home::draw::arc::ArcSCLCommand;
                let new_cmd = ArcSCLCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_SEA" => {
                use crate::modules::home::draw::arc::ArcSEACommand;
                let new_cmd = ArcSEACommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_SER" => {
                use crate::modules::home::draw::arc::ArcSERCommand;
                let new_cmd = ArcSERCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_SED" => {
                use crate::modules::home::draw::arc::ArcSEDCommand;
                let new_cmd = ArcSEDCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_CSA" => {
                use crate::modules::home::draw::arc::ArcCSACommand;
                let new_cmd = ArcCSACommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "ARC_CSL" => {
                use crate::modules::home::draw::arc::ArcCSLCommand;
                let new_cmd = ArcCSLCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "RECT"|"RECTANG"|"REC" => {
                use crate::modules::home::draw::shapes::RectCommand;
                let new_cmd = RectCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "RECT_ROT" => {
                use crate::modules::home::draw::shapes::RectRotCommand;
                let new_cmd = RectRotCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "RECT_CEN" => {
                use crate::modules::home::draw::shapes::RectCenCommand;
                let new_cmd = RectCenCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "POLY"|"POLYGON"|"POL" => {
                use crate::modules::home::draw::shapes::PolyCommand;
                let new_cmd = PolyCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "POLY_C" => {
                use crate::modules::home::draw::shapes::PolyCCommand;
                let new_cmd = PolyCCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }
            "POLY_E" => {
                use crate::modules::home::draw::shapes::PolyECommand;
                let new_cmd = PolyECommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "PLINE"|"PL" => {
                use crate::modules::home::draw::polyline::PlineCommand;
                let new_cmd = PlineCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            // ── Modify commands ────────────────────────────────────────────
            "MOVE"|"M" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("MOVE");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::translate::MoveCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = MoveCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "COPY"|"CO" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("COPY");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::copy::CopyCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = CopyCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "ROTATE"|"RO" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("ROTATE");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::rotate::RotateCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = RotateCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "POINT"|"PO" => {
                use crate::modules::home::draw::point::PointCommand;
                let new_cmd = PointCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "RAY" => {
                use crate::modules::home::draw::ray::RayCommand;
                let new_cmd = RayCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "XLINE"|"XL"|"CONSTRUCTIONLINE" => {
                use crate::modules::home::draw::ray::XLineCommand;
                let new_cmd = XLineCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "HATCH"|"H" => {
                use crate::modules::home::draw::hatch::HatchCommand;
                let outlines = self.tabs[i].scene.closed_outlines();
                let new_cmd = HatchCommand::new(outlines);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "HATCHEDIT"|"HE" => {
                use crate::modules::home::draw::hatchedit::HatcheditCommand;
                // If a single hatch is already selected, skip the pick step.
                let sel = self.tabs[i].scene.selected_entities();
                if sel.len() == 1 {
                    let (h, _) = sel[0];
                    if let Some(model) = self.tabs[i].scene.hatches.get(&h).cloned() {
                        let cmd = HatcheditCommand::with_handle(
                            h, model.name.clone(), model.scale, model.angle_offset,
                        );
                        self.command_line.push_info(&cmd.prompt());
                        self.tabs[i].active_cmd = Some(Box::new(cmd));
                    } else {
                        self.command_line.push_error("HATCHEDIT: selected entity is not a hatch.");
                    }
                } else {
                    let cmd = HatcheditCommand::new();
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "GRADIENT" => {
                use crate::modules::home::draw::hatch::GradientCommand;
                let outlines = self.tabs[i].scene.closed_outlines();
                let new_cmd = GradientCommand::new(outlines);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "BOUNDARY" => {
                use crate::modules::home::draw::hatch::BoundaryCommand;
                let outlines = self.tabs[i].scene.closed_outlines();
                let new_cmd = BoundaryCommand::new(outlines);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "ELLIPSE"|"EL" => {
                use crate::modules::home::draw::ellipse::EllipseCommand;
                let new_cmd = EllipseCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "ELLIPSE_AXIS" => {
                use crate::modules::home::draw::ellipse::EllipseAxisCommand;
                let new_cmd = EllipseAxisCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "ELLIPSE_ARC" => {
                use crate::modules::home::draw::ellipse::EllipseArcCommand;
                let new_cmd = EllipseArcCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "SPLINE"|"SPL" => {
                use crate::modules::home::draw::spline::SplineCommand;
                let new_cmd = SplineCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "SCALE"|"SC" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("SCALE");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::scale::ScaleCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = ScaleCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "MIRROR"|"MI" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("MIRROR");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::mirror::MirrorCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = MirrorCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "ERASE"|"E" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("ERASE");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let n = handles.len();
                    self.push_undo_snapshot(i, "ERASE");
                    self.tabs[i].scene.erase_entities(&handles);
                    self.tabs[i].dirty = true;
                    self.refresh_properties();
                    self.command_line.push_output(&format!("{n} object(s) erased."));
                }
            }

            // ── Annotate commands ──────────────────────────────────────────
            "TEXT"|"T"|"DT" => {
                use crate::modules::annotate::text::TextCommand;
                let new_cmd = TextCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "DDEDIT"|"ED" => {
                use crate::modules::annotate::ddedit::{DdeditCommand, entity_text};
                // If a single text/mtext entity is already selected, skip the pick step.
                let sel = self.tabs[i].scene.selected_entities();
                if sel.len() == 1 {
                    let (h, _) = sel[0];
                    if let Some(e) = self.tabs[i].scene.document.get_entity(h) {
                        if let Some(cur) = entity_text(e) {
                            let cmd = DdeditCommand::with_handle(h, cur.clone());
                            self.command_line.push_info(&format!("DDEDIT  Enter new text <{cur}>:"));
                            self.tabs[i].active_cmd = Some(Box::new(cmd));
                        } else {
                            self.command_line.push_error("DDEDIT: selected entity is not text.");
                        }
                    }
                } else {
                    let cmd = DdeditCommand::new();
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "MTEXT"|"MT" => {
                use crate::modules::annotate::mtext::MTextCommand;
                let new_cmd = MTextCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "DIMALIGNED"|"DAL" => {
                use crate::modules::annotate::aligned_dim::AlignedDimensionCommand;
                let cmd = AlignedDimensionCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMDIAMETER"|"DDI" => {
                use crate::modules::annotate::diameter_dim::DiameterDimensionCommand;
                let cmd = DiameterDimensionCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMLINEAR" => {
                use crate::modules::annotate::linear_dim::LinearDimensionCommand;
                let new_cmd = LinearDimensionCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "DIMRADIUS" => {
                use crate::modules::annotate::radius_dim::RadiusDimensionCommand;
                let new_cmd = RadiusDimensionCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "DIMANGULAR" => {
                use crate::modules::annotate::angular_dim::AngularDimensionCommand;
                let new_cmd = AngularDimensionCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "DIMORDINATE"|"DOR" => {
                use crate::modules::annotate::ordinate_dim::OrdinateDimCommand;
                let new_cmd = OrdinateDimCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "LEADER"|"LE" => {
                use crate::modules::annotate::leader_cmd::LeaderCommand;
                let new_cmd = LeaderCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "MLEADER"|"MLD" => {
                use crate::modules::annotate::mleader_cmd::MLeaderCommand;
                let new_cmd = MLeaderCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "TOLERANCE"|"TOL" => {
                use crate::modules::annotate::tolerance_cmd::ToleranceCommand;
                let cmd = ToleranceCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "TABLE" => {
                use crate::modules::annotate::table_cmd::TableCommand;
                let cmd = TableCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMCONTINUE"|"DCO" => {
                use crate::modules::annotate::dim_continue::DimContinueCommand;
                let cmd = if let Some((p1, p2, dp, rot)) = find_last_linear_dim(&self.tabs[i].scene) {
                    DimContinueCommand::from_base(p1, p2, dp, rot)
                } else {
                    DimContinueCommand::new()
                };
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMBASELINE"|"DBA" => {
                use crate::modules::annotate::dim_baseline::DimBaselineCommand;
                let cmd = if let Some((p1, p2, dp, rot)) = find_last_linear_dim(&self.tabs[i].scene) {
                    DimBaselineCommand::from_base(p1, p2, dp, rot)
                } else {
                    DimBaselineCommand::new()
                };
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "QDIM" => {
                use crate::modules::annotate::qdim::QdimCommand;
                let cmd = QdimCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMEDIT"|"DED" => {
                use crate::modules::annotate::dimedit::DimEditCommand;
                let cmd = DimEditCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMTEDIT"|"DIMTED" => {
                use crate::modules::annotate::dimtedit::DimTeditCommand;
                let cmd = DimTeditCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMBREAK"|"DBR" => {
                use crate::modules::annotate::dimbreak::DimBreakCommand;
                let cmd = DimBreakCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMSPACE"|"DSPACE" => {
                use crate::modules::annotate::dimspace::DimSpaceCommand;
                let cmd = DimSpaceCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIMJOGLINE"|"DJL" => {
                use crate::modules::annotate::dimjogline::DimJogLineCommand;
                let cmd = DimJogLineCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "MLEADERADD"|"MLA" => {
                use crate::modules::annotate::mleader_edit::MLeaderAddCommand;
                let cmd = MLeaderAddCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "MLEADERREMOVE"|"MLR" => {
                use crate::modules::annotate::mleader_edit::MLeaderRemoveCommand;
                let cmd = MLeaderRemoveCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "MLEADERALIGN"|"MLAL" => {
                use crate::modules::annotate::mleader_edit::MLeaderAlignCommand;
                let cmd = MLeaderAlignCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "MLEADERCOLLECT"|"MLC" => {
                use crate::modules::annotate::mleader_edit::MLeaderCollectCommand;
                let cmd = MLeaderCollectCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "ZOOM EXTENTS"|"ZOOMEXTENTS"|"ZE" => {
                self.tabs[i].scene.fit_all();
                self.command_line.push_output("Zoom Extents");
            }

            "ZOOM IN"|"ZI" => {
                self.tabs[i].scene.zoom_camera(1.0 / 1.5);
                self.command_line.push_output("Zoom In");
            }

            "ZOOM OUT"|"ZO" => {
                self.tabs[i].scene.zoom_camera(1.5);
                self.command_line.push_output("Zoom Out");
            }

            // ZOOM ALL — fit all entities (same as EXTENTS for now)
            "ZOOM ALL"|"ZOOM A"|"ZA" => {
                self.tabs[i].scene.fit_all();
                self.command_line.push_output("Zoom All");
            }

            // ZOOM SCALE — set zoom factor (e.g. "ZOOM SCALE 2" or "ZS 0.5")
            cmd if cmd.starts_with("ZOOM SCALE ") || cmd.starts_with("ZS ") => {
                let rest = cmd.split_once(' ')
                    .and_then(|(_, r)| r.split_once(' ').map(|(_, v)| v).or(Some(r)))
                    .unwrap_or("1");
                if let Ok(factor) = rest.trim().parse::<f32>() {
                    if factor > 0.0 {
                        self.tabs[i].scene.zoom_camera(1.0 / factor);
                        self.command_line.push_output(&format!("Zoom Scale ×{factor:.3}"));
                    }
                }
            }

            "PLOTWINDOW"|"PW" => {
                use crate::modules::view::plot_window::PlotWindowCommand;
                let cmd = PlotWindowCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "ZOOM WINDOW"|"ZOOM W"|"ZW" => {
                use crate::modules::view::zoom_window::ZoomWindowCommand;
                let new_cmd = ZoomWindowCommand::new();
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "STRETCH"|"SS" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("STRETCH");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::stretch::StretchCommand;
                    let new_cmd = StretchCommand::new(handles);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "FILLET"|"F" => {
                use crate::modules::home::modify::fillet::FilletCommand;
                let entities: Vec<_> = self.tabs[i].scene.entity_wires().iter()
                    .filter_map(|w| {
                        let h = Scene::handle_from_wire_name(&w.name)?;
                        self.tabs[i].scene.document.get_entity(h).cloned().map(|e| (h, e))
                    }).collect();
                let all_entities: Vec<_> = entities.into_iter().map(|(_, e)| e).collect();
                let new_cmd = FilletCommand::new(
                    crate::modules::home::defaults::get_fillet_radius(), all_entities);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "ARRAY"|"AR"|"ARRAYRECT" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("ARRAYRECT");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::array::ArrayRectCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = ArrayRectCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "ARRAYPOLAR" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("ARRAYPOLAR");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::array::ArrayPolarCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let new_cmd = ArrayPolarCommand::new(handles, wires);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "ARRAYPATH" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("ARRAYPATH");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::array::ArrayPathCommand;
                    let wires = self.tabs[i].scene.wire_models_for(&handles);
                    let all_entities: Vec<_> = self.tabs[i].scene.entity_wires().iter()
                        .filter_map(|w| {
                            let h = Scene::handle_from_wire_name(&w.name)?;
                            self.tabs[i].scene.document.get_entity(h).cloned()
                        }).collect();
                    let new_cmd = ArrayPathCommand::new(handles, wires, all_entities);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "ARRAY3D"|"3DARRAY" => {
                let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().map(|(h, _)| h).collect();
                if handles.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("ARRAY3D");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::home::modify::array::Array3DCommand;
                    let new_cmd = Array3DCommand::new(handles);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            "CHAMFER"|"CHA" => {
                use crate::modules::home::modify::fillet::ChamferCommand;
                let entities: Vec<_> = self.tabs[i].scene.entity_wires().iter()
                    .filter_map(|w| {
                        let h = Scene::handle_from_wire_name(&w.name)?;
                        self.tabs[i].scene.document.get_entity(h).cloned().map(|e| (h, e))
                    }).collect();
                let all_entities: Vec<_> = entities.into_iter().map(|(_, e)| e).collect();
                let new_cmd = ChamferCommand::new(
                    crate::modules::home::defaults::get_chamfer_dist1(), all_entities);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "EXPLODE"|"X" => {
                use crate::modules::home::modify::explode::explode_entity;
                let entities: Vec<_> = self.tabs[i].scene.selected_entities()
                    .into_iter().collect();
                if entities.is_empty() {
                    use crate::modules::home::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("EXPLODE");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let replacements: Vec<(acadrust::Handle, Vec<acadrust::EntityType>)> = entities
                        .iter()
                        .filter_map(|(h, e)| {
                            let pieces = explode_entity(e, &self.tabs[i].scene.document);
                            if pieces.is_empty() { None } else { Some((*h, pieces)) }
                        })
                        .collect();
                    let exploded = replacements.len();
                    if exploded > 0 {
                        self.push_undo_snapshot(i, "EXPLODE");
                    }
                    for (handle, pieces) in replacements {
                        self.tabs[i].scene.erase_entities(&[handle]);
                        for piece in pieces {
                            self.tabs[i].scene.add_entity(piece);
                        }
                    }
                    if exploded > 0 {
                        self.tabs[i].dirty = true;
                        self.refresh_properties();
                        self.command_line.push_output(&format!("{exploded} object(s) exploded."));
                    } else {
                        self.command_line.push_info("EXPLODE: no explodable objects selected.");
                    }
                }
            }

            "OFFSET"|"O" => {
                use crate::modules::home::modify::offset::OffsetCommand;
                let all_entities: Vec<_> = self.tabs[i].scene.entity_wires().iter()
                    .filter_map(|w| {
                        let h = Scene::handle_from_wire_name(&w.name)?;
                        self.tabs[i].scene.document.get_entity(h).cloned()
                    }).collect();
                let new_cmd = OffsetCommand::new(
                    crate::modules::home::defaults::get_offset_dist(), all_entities);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "TRIM"|"TR" => {
                use crate::modules::home::modify::trim::TrimCommand;
                let entities: Vec<_> = self.tabs[i].scene.entity_wires().iter()
                    .filter_map(|w| {
                        let h = Scene::handle_from_wire_name(&w.name)?;
                        self.tabs[i].scene.document.get_entity(h).cloned().map(|e| (h, e))
                    }).collect();
                let all_entities: Vec<_> = entities.into_iter().map(|(_, e)| e).collect();
                let new_cmd = TrimCommand::new(all_entities);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "EXTEND"|"EX" => {
                use crate::modules::home::modify::trim::ExtendCommand;
                let entities: Vec<_> = self.tabs[i].scene.entity_wires().iter()
                    .filter_map(|w| {
                        let h = Scene::handle_from_wire_name(&w.name)?;
                        self.tabs[i].scene.document.get_entity(h).cloned().map(|e| (h, e))
                    }).collect();
                let all_entities: Vec<_> = entities.into_iter().map(|(_, e)| e).collect();
                let new_cmd = ExtendCommand::new(all_entities);
                self.command_line.push_info(&new_cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(new_cmd));
            }

            "3DORBIT"|"3O" => {
                self.command_line.push_info("3D Orbit: drag with right mouse button.");
            }

            // ── Selection utilities ───────────────────────────────────────
            "SELECTALL"|"SA" => {
                use crate::scene::Scene;
                let handles: Vec<acadrust::Handle> = self.tabs[i].scene.entity_wires()
                    .iter()
                    .filter_map(|w| Scene::handle_from_wire_name(&w.name))
                    .collect();
                let count = handles.len();
                for h in handles {
                    self.tabs[i].scene.select_entity(h, false);
                }
                self.command_line.push_output(&format!("SELECTALL: {} object(s) selected.", count));
                self.refresh_properties();
            }

            "DESELECT"|"DE"|"DESELALL" => {
                self.tabs[i].scene.deselect_all();
                self.command_line.push_output("Deselected.");
                self.refresh_properties();
            }

            // ── LIST — entity info ────────────────────────────────────────
            "LIST" | "LI" => {
                let selected: Vec<_> = self.tabs[i].scene.selected_entities();
                if selected.is_empty() {
                    self.command_line.push_error("LIST: no entities selected. Select entities first.");
                } else {
                    for (handle, _) in &selected {
                        if let Some(entity) = self.tabs[i].scene.document.get_entity(*handle) {
                            let type_name = entity_type_name(entity);
                            let common = entity.common();
                            let color_str = common.color.index()
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "ByLayer".to_string());
                            let linetype = if common.linetype.is_empty() || common.linetype == "ByLayer" {
                                "ByLayer".to_string()
                            } else {
                                common.linetype.clone()
                            };
                            // Entity-specific details
                            let details = entity_list_details(entity);
                            self.command_line.push_output(&format!(
                                "{type_name}  Handle:{:X}  Layer:{}  Color:{}  LT:{}{}",
                                handle.value(), common.layer, color_str, linetype,
                                if details.is_empty() { String::new() } else { format!("\n    {details}") }
                            ));
                        }
                    }
                }
            }

            // ── Break / Join ─────────────────────────────────────────────────
            "JOIN"|"J" => {
                use crate::modules::home::modify::join::JoinCommand;
                let cmd = JoinCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "BREAK"|"BR" => {
                use crate::modules::home::modify::break_cmd::BreakInteractiveCommand;
                let cmd = BreakInteractiveCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "BREAKATPOINT"|"BAP" => {
                use crate::modules::home::modify::break_cmd::BreakAtPointCommand;
                let cmd = BreakAtPointCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "PEDIT"|"PE" => {
                use crate::modules::home::modify::pedit::PeditCommand;
                let cmd_obj = PeditCommand::new();
                self.command_line.push_info(&cmd_obj.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
            }

            "SPLINEDIT"|"SPE" => {
                use crate::modules::home::modify::splinedit::SplineditCommand;
                let cmd_obj = SplineditCommand::new();
                self.command_line.push_info(&cmd_obj.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
            }

            "ATTEDIT"|"ATE"|"-ATTEDIT" => {
                use crate::modules::home::modify::attedit::AtteditCommand;
                let cmd_obj = AtteditCommand::new();
                self.command_line.push_info(&cmd_obj.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
            }

            // ── REFEDIT — in-place block editing ─────────────────────────────
            "REFEDIT" => {
                use crate::modules::home::modify::refedit::RefEditPickCommand;
                // If a session is already active, tell the user.
                if self.tabs[i].refedit_session.is_some() {
                    self.command_line.push_error(
                        "REFEDIT: a session is already active. Use REFCLOSE first."
                    );
                } else {
                    // Check if a single INSERT is already selected.
                    let selected: Vec<_> = self.tabs[i].scene.selected_entities().into_iter().collect();
                    if selected.len() == 1 {
                        if let Some(acadrust::EntityType::Insert(_)) = selected.first().map(|(_, e)| e) {
                            let handle = selected[0].0;
                            // Skip pick phase — jump straight to begin.
                            let _ = self.dispatch_command(&format!("REFEDIT_BEGIN:{}", handle.value()));
                            return Task::none();
                        }
                    }
                    let cmd_obj = RefEditPickCommand::new();
                    self.command_line.push_info(&cmd_obj.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
                }
            }

            cmd if cmd.starts_with("REFEDIT_BEGIN:") => {
                use crate::modules::home::modify::refedit::{RefEditSession, apply_insert_transform};
                use acadrust::Handle;

                let handle_u64: u64 = cmd["REFEDIT_BEGIN:".len()..]
                    .parse().unwrap_or(0);
                let insert_handle = Handle::new(handle_u64);

                // Get INSERT entity.
                let insert = match self.tabs[i].scene.document.get_entity(insert_handle) {
                    Some(acadrust::EntityType::Insert(ins)) => ins.clone(),
                    _ => {
                        self.command_line.push_error("REFEDIT: selected object is not an INSERT.");
                        return Task::none();
                    }
                };

                // Validate: non-uniform scale is not supported.
                let sx = insert.x_scale();
                let sy = insert.y_scale();
                let sz = insert.z_scale();
                if (sx - sy).abs() > 1e-6 || (sx - sz).abs() > 1e-6 {
                    self.command_line.push_error(
                        "REFEDIT: non-uniform scale inserts are not supported."
                    );
                    return Task::none();
                }

                // Find the block record.
                let br_handle = match self.tabs[i].scene.document.block_records.get(&insert.block_name) {
                    Some(br) => br.handle,
                    None => {
                        self.command_line.push_error(&format!(
                            "REFEDIT: block \"{}\" not found.", insert.block_name
                        ));
                        return Task::none();
                    }
                };

                // Collect block-local entities (skip structural Block/BlockEnd/AttDef).
                let block_entities: Vec<_> = {
                    let br = self.tabs[i].scene.document.block_records.get(&insert.block_name).unwrap();
                    br.entity_handles
                        .iter()
                        .filter_map(|h| self.tabs[i].scene.document.get_entity(*h).cloned())
                        .filter(|e| !matches!(e,
                            acadrust::EntityType::Block(_) |
                            acadrust::EntityType::BlockEnd(_) |
                            acadrust::EntityType::AttributeDefinition(_)
                        ))
                        .collect()
                };

                if block_entities.is_empty() {
                    self.command_line.push_error("REFEDIT: block is empty.");
                    return Task::none();
                }

                let session = RefEditSession {
                    insert_handle,
                    block_name: insert.block_name.clone(),
                    br_handle,
                    temp_handles: vec![],
                    insert_x: insert.insert_point.x,
                    insert_y: insert.insert_point.y,
                    insert_z: insert.insert_point.z,
                    rotation_deg: insert.rotation.to_degrees(),
                    scale: sx,
                };

                self.push_undo_snapshot(i, "REFEDIT");
                self.tabs[i].refedit_session = Some(session.clone());

                // Add block entities to model space with INSERT transform applied.
                let mut temp_handles = Vec::new();
                for mut entity in block_entities {
                    apply_insert_transform(&mut entity, &session);
                    entity.common_mut().handle = acadrust::Handle::NULL;
                    entity.common_mut().owner_handle = acadrust::Handle::NULL;
                    let h = self.tabs[i].scene.add_entity(entity);
                    temp_handles.push(h);
                }
                self.tabs[i].refedit_session.as_mut().unwrap().temp_handles = temp_handles.clone();

                // Select the temp entities so user can see what they're editing.
                self.tabs[i].scene.deselect_all();
                for h in &temp_handles {
                    self.tabs[i].scene.select_entity(*h, false);
                }
                self.tabs[i].dirty = true;

                self.command_line.push_info(&format!(
                    "REFEDIT: Editing block \"{}\". Use REFCLOSE when done.",
                    insert.block_name
                ));
                use crate::modules::home::modify::refedit::RefCloseCommand;
                let cmd_obj = RefCloseCommand::new();
                self.command_line.push_info(&cmd_obj.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
            }

            "REFCLOSE" => {
                if self.tabs[i].refedit_session.is_some() {
                    use crate::modules::home::modify::refedit::RefCloseCommand;
                    let cmd_obj = RefCloseCommand::new();
                    self.command_line.push_info(&cmd_obj.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd_obj));
                } else {
                    self.command_line.push_error("REFCLOSE: no REFEDIT session active.");
                }
            }

            "REFCLOSE_SAVE" => {
                use crate::modules::home::modify::refedit::apply_insert_inverse_transform;
                use crate::modules::home::modify::explode::normalize_entity_for_block;

                let session = match self.tabs[i].refedit_session.take() {
                    Some(s) => s,
                    None => {
                        self.command_line.push_error("REFCLOSE: no REFEDIT session active.");
                        return Task::none();
                    }
                };

                self.push_undo_snapshot(i, "REFCLOSE");

                // Collect the edited temp entities.
                let new_entities: Vec<acadrust::EntityType> = session.temp_handles
                    .iter()
                    .filter_map(|h| self.tabs[i].scene.document.get_entity(*h).cloned())
                    .collect();

                // Remove temp entities from model space.
                self.tabs[i].scene.erase_entities(&session.temp_handles);

                // Apply inverse INSERT transform → block-local coordinates.
                let new_entities: Vec<_> = new_entities
                    .into_iter()
                    .map(|mut entity| {
                        apply_insert_inverse_transform(&mut entity, &session);
                        let mut entity = normalize_entity_for_block(entity);
                        entity.common_mut().handle = acadrust::Handle::NULL;
                        entity.common_mut().owner_handle = session.br_handle;
                        entity
                    })
                    .collect();

                // Remove old block entities from the document.
                let old_handles: Vec<_> = match self.tabs[i].scene.document
                    .block_records.get(&session.block_name)
                {
                    Some(br) => br.entity_handles.clone(),
                    None => vec![],
                };
                for h in &old_handles {
                    self.tabs[i].scene.document.remove_entity(*h);
                }
                // Flush the entity_handles list from the block record.
                if let Some(br) = self.tabs[i].scene.document
                    .block_records.get_mut(&session.block_name)
                {
                    br.entity_handles.clear();
                }

                // Add the new block entities.
                for entity in new_entities {
                    let _ = self.tabs[i].scene.document.add_entity(entity);
                }

                self.tabs[i].dirty = true;
                self.command_line.push_output(&format!(
                    "REFCLOSE: Block \"{}\" saved. All references updated.",
                    session.block_name
                ));
                // Rebuild hatch/image/mesh caches since block content changed.
                self.tabs[i].scene.rebuild_derived_caches();
            }

            "REFCLOSE_DISCARD" => {
                let session = match self.tabs[i].refedit_session.take() {
                    Some(s) => s,
                    None => {
                        self.command_line.push_error("REFCLOSE: no REFEDIT session active.");
                        return Task::none();
                    }
                };
                // Remove temp entities without modifying the block.
                self.tabs[i].scene.erase_entities(&session.temp_handles);
                self.tabs[i].scene.deselect_all();
                self.command_line.push_output("REFCLOSE: Changes discarded.");
            }

            "ALIGN"|"AL" => {
                use crate::modules::home::modify::align::AlignCommand;
                let cmd = AlignCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "LENGTHEN"|"LEN" => {
                use crate::modules::home::modify::lengthen::LengthenCommand;
                let cmd = LengthenCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "DIVIDE"|"DIV" => {
                use crate::modules::home::inquiry::divide::DivideCommand;
                let cmd = DivideCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "MEASURE"|"ME" => {
                use crate::modules::home::inquiry::divide::MeasureCommand;
                let cmd = MeasureCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── Inquiry ──────────────────────────────────────────────────────
            "DIST"|"DI" => {
                use crate::modules::home::inquiry::dist::DistCommand;
                let cmd = DistCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "ID" => {
                use crate::modules::home::inquiry::id::IdCommand;
                let cmd = IdCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            "AREA" => {
                use crate::modules::home::inquiry::area::AreaCommand;
                let cmd = AreaCommand::new();
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── MASSPROP — area, perimeter, centroid of selected entities ────
            "MASSPROP" => {
                let selected = self.tabs[i].scene.selected_entities();
                if selected.is_empty() {
                    self.command_line.push_error(
                        "MASSPROP: no entities selected. Select entities first."
                    );
                } else {
                    for (handle, _) in &selected {
                        if let Some(entity) = self.tabs[i].scene.document.get_entity(*handle) {
                            if let Some(props) = massprop_entity(entity) {
                                self.command_line.push_output(&format!(
                                    "{}  Area={:.4}  Perimeter={:.4}  Centroid=({:.4},{:.4})",
                                    entity_type_name(entity),
                                    props.area,
                                    props.perimeter,
                                    props.cx,
                                    props.cy,
                                ));
                            }
                        }
                    }
                }
            }

            // ── FLATTEN — move selected (or all) entities to Z=0 ─────────────
            "FLATTEN" => {
                let handles: Vec<acadrust::Handle> = {
                    let sel = self.tabs[i].scene.selected_entities();
                    if sel.is_empty() {
                        // Flatten all entities
                        self.tabs[i].scene.document.entities()
                            .map(|e| e.common().handle)
                            .collect()
                    } else {
                        sel.into_iter().map(|(h, _)| h).collect()
                    }
                };
                if handles.is_empty() {
                    self.command_line.push_error("FLATTEN: no entities.");
                } else {
                    self.push_undo_snapshot(i, "FLATTEN");
                    for h in &handles {
                        if let Some(e) = self.tabs[i].scene.document.get_entity_mut(*h) {
                            flatten_entity_z(e);
                        }
                    }
                    self.tabs[i].dirty = true;
                    self.command_line.push_output(&format!(
                        "FLATTEN: {} entity(ies) moved to Z=0.", handles.len()
                    ));
                    self.refresh_properties();
                }
            }

            // ── QSELECT — quick-select entities by property ───────────────────
            // QSELECT TYPE <type>          — select all entities of given type
            // QSELECT LAYER <name>         — select all entities on layer
            // QSELECT COLOR <n>            — select all entities with color index n
            // QSELECT LINETYPE <name>      — select all entities with linetype
            cmd if cmd == "QSELECT" || cmd.starts_with("QSELECT ") => {
                let rest = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                let prop = parts.first().map(|s| s.to_uppercase()).unwrap_or_default();
                let val  = parts.get(1).map(|s| s.trim()).unwrap_or("").to_uppercase();

                let matched: Vec<acadrust::Handle> = self.tabs[i].scene.document
                    .entities()
                    .filter(|e| {
                        let c = e.common();
                        match prop.as_str() {
                            "TYPE"     => entity_type_name(e).to_uppercase() == val,
                            "LAYER"    => c.layer.to_uppercase() == val,
                            "COLOR"    => c.color.index()
                                .map(|n| n.to_string() == val)
                                .unwrap_or(val == "BYLAYER"),
                            "LINETYPE" => c.linetype.to_uppercase() == val,
                            _ => false,
                        }
                    })
                    .map(|e| e.common().handle)
                    .collect();

                if prop.is_empty() {
                    self.command_line.push_info(
                        "Usage: QSELECT TYPE|LAYER|COLOR|LINETYPE <value>"
                    );
                } else if matched.is_empty() {
                    self.command_line.push_output("QSELECT: no matching entities.");
                } else {
                    self.tabs[i].scene.deselect_all();
                    for h in &matched {
                        self.tabs[i].scene.select_entity(*h, false);
                    }
                    self.command_line.push_output(&format!(
                        "QSELECT: {} entity(ies) selected.", matched.len()
                    ));
                    self.refresh_properties();
                }
            }

            // ── COUNT — entity statistics ─────────────────────────────────────
            cmd if cmd == "COUNT" || cmd.starts_with("COUNT ") => {
                let filter = cmd.split_once(' ').map(|(_, r)| r.trim().to_uppercase());
                let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
                for e in self.tabs[i].scene.document.entities() {
                    let layer = &e.common().layer;
                    let type_name = entity_type_name(e);
                    let key = match &filter {
                        Some(f) if f == "LAYER" => layer.clone(),
                        Some(f) if f == "TYPE"  => type_name.to_string(),
                        Some(f) => {
                            // Filter by layer name
                            if layer.to_uppercase() != *f { continue; }
                            type_name.to_string()
                        }
                        None => type_name.to_string(),
                    };
                    *counts.entry(key).or_default() += 1;
                }
                let total: usize = counts.values().sum();
                for (k, n) in &counts {
                    self.command_line.push_output(&format!("  {k}: {n}"));
                }
                self.command_line.push_output(&format!("COUNT: {total} entity(ies) total."));
            }

            "DATAEXTRACTION" | "EATTEXT" | "ATTEXT" => {
                let csv = build_data_extraction_csv(&self.tabs[i].scene.document);
                return Task::done(Message::DataExtractionSave(csv));
            }

            // ── Find / Replace ────────────────────────────────────────────────
            // FIND <search>              — list all Text/MText/Dimension containing <search>
            // FIND <search> REPLACE <rep> — replace first occurrence (case-insensitive)
            // FINDALL <search> REPLACE <rep> — replace all occurrences
            cmd if cmd == "FIND" || cmd.starts_with("FIND ") || cmd == "FINDALL" || cmd.starts_with("FINDALL ") => {
                let all_mode = cmd.starts_with("FINDALL");
                let rest = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");

                // Split at " REPLACE " keyword (case-insensitive)
                let (search, replacement) = if let Some(pos) = rest.to_uppercase().find(" REPLACE ") {
                    (&rest[..pos], Some(rest[pos + 9..].trim()))
                } else {
                    (rest, None)
                };

                if search.is_empty() {
                    self.command_line.push_error("FIND: specify search text.");
                } else {
                    let search_lc = search.to_lowercase();
                    let mut count = 0usize;
                    let handles: Vec<acadrust::Handle> = self.tabs[i].scene.document
                        .entities()
                        .filter_map(|e| {
                            let txt = entity_text_content(e)?;
                            if txt.to_lowercase().contains(&search_lc) {
                                Some(e.common().handle)
                            } else {
                                None
                            }
                        })
                        .collect();

                    if let Some(rep) = replacement {
                        // Replace mode
                        let targets: Vec<_> = if all_mode {
                            handles.clone()
                        } else {
                            handles.iter().copied().take(1).collect()
                        };
                        if targets.is_empty() {
                            self.command_line.push_output(&format!("FIND: \"{}\" not found.", search));
                        } else {
                            self.push_undo_snapshot(i, "FIND/REPLACE");
                            for h in &targets {
                                if let Some(e) = self.tabs[i].scene.document.get_entity_mut(*h) {
                                    replace_entity_text(e, search, rep);
                                    count += 1;
                                }
                            }
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!(
                                "FIND/REPLACE: replaced {} occurrence(s) of \"{}\" → \"{}\".",
                                count, search, rep
                            ));
                            self.refresh_properties();
                        }
                    } else {
                        // List mode
                        if handles.is_empty() {
                            self.command_line.push_output(&format!("FIND: \"{}\" not found.", search));
                        } else {
                            for h in &handles {
                                if let Some(e) = self.tabs[i].scene.document.get_entity(*h) {
                                    let txt = entity_text_content(e).unwrap_or_default();
                                    self.command_line.push_output(&format!(
                                        "  Handle {:X}: \"{}\"", h.value(), txt
                                    ));
                                }
                            }
                            self.command_line.push_output(&format!(
                                "FIND: {} match(es) for \"{}\".", handles.len(), search
                            ));
                        }
                    }
                }
            }

            "HELP"|"?" => {
                self.command_line.push_output(
                    "Draw: LINE CIRCLE ARC PLINE RECTANG(RECT) POLYGON(POLY) POINT ELLIPSE SPLINE RAY XLINE HATCH DONUT REVCLOUD WIPEOUT MLINE ATTDEF  |  \
                     Modify: MOVE COPY ROTATE SCALE MIRROR ERASE OFFSET EXTEND FILLET CHAMFER STRETCH EXPLODE TRIM BREAK JOIN LENGTHEN ALIGN PEDIT  |  \
                     Array: ARRAY ARRAYRECT ARRAYPOLAR ARRAYPATH  |  \
                     Text: TEXT MTEXT LEADER MLEADER  |  \
                     Dimension: DIMLINEAR DIMALIGNED DIMANGULAR DIMRADIUS DIMDIAMETER DIMCONTINUE DIMBASELINE  |  \
                     Annotation: TOLERANCE  |  \
                     Inquiry: DIST ID AREA LIST FIND FINDALL COUNT QSELECT  |  Draw on entity: DIVIDE MEASURE  |  \
                     Attributes: ATTEDIT ATTDISP  |  \
                     Utilities: FLATTEN LAYISO LAYUNISO  |  \
                     View: ZOOM EXTENTS ZOOM WINDOW VIEW LIST/SAVE/RESTORE/DELETE  |  \
                     Layer: LAYER LIST/NEW/ON/OFF/FREEZE/THAW/LOCK/UNLOCK/COLOR/SET  |  \
                     Viewport: MVIEW VPLAYER VPORTS MS PS DRAWORDER  |  \
                     Tables: STYLE DIMSTYLE LINETYPE UCS RENAME PURGE  |  \
                     File: NEW OPEN SAVE SAVEAS PRINT PURGE UNDO REDO"
                );
            }

            "DONATE" => {
                let _ = open::that("https://patreon.com/HakanSeven12");
                self.command_line.push_info("Opening Patreon page...");
            }

            "REPORT" => {
                let _ = open::that("https://github.com/HakanSeven12/H7CAD/issues/new");
                self.command_line.push_info("Opening GitHub issue page...");
            }

            "ABOUT" => {
                return Task::done(Message::AboutOpen);
            }

            "CHANGELOG" => {
                let _ = open::that("https://github.com/HakanSeven12/H7CAD/releases");
                self.command_line.push_info("Opening release notes...");
            }

            // ── Keyboard Shortcuts panel ──────────────────────────────────
            cmd if cmd == "SHORTCUTS" || cmd.starts_with("SHORTCUTS ") => {
                let raw_rest = cmd.trim_start_matches("SHORTCUTS").trim();
                let parts: Vec<&str> = raw_rest.splitn(3, ' ').collect();
                let sub = parts.first().map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "LIST" | "?" => {
                        return Task::done(Message::ShortcutsPanelOpen);
                    }
                    "SET" | "S" => {
                        // SHORTCUTS SET <key> <command>
                        // e.g. SHORTCUTS SET CTRL+D DIST
                        let key = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
                        let cmd_str = parts.get(2).map(|s| s.to_uppercase()).unwrap_or_default();
                        if key.is_empty() || cmd_str.is_empty() {
                            self.command_line.push_error("Usage: SHORTCUTS SET <key> <command>  e.g. SHORTCUTS SET CTRL+D DIST");
                        } else {
                            self.shortcut_overrides.insert(key.clone(), cmd_str.clone());
                            self.command_line.push_output(&format!("Shortcut set: {key} → {cmd_str}"));
                        }
                    }
                    "CLEAR" | "DELETE" | "REMOVE" => {
                        let key = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
                        if key.is_empty() {
                            self.command_line.push_error("Usage: SHORTCUTS CLEAR <key>");
                        } else if self.shortcut_overrides.remove(&key).is_some() {
                            self.command_line.push_output(&format!("Shortcut '{key}' removed."));
                        } else {
                            self.command_line.push_error(&format!("Shortcut '{key}' not found."));
                        }
                    }
                    _ => {
                        self.command_line.push_info("Usage: SHORTCUTS LIST | SET <key> <cmd> | CLEAR <key>");
                    }
                }
            }

            // ── Color Scheme / Theme selector ─────────────────────────────
            cmd if cmd == "COLORSCHEME" || cmd.starts_with("COLORSCHEME ") => {
                use iced::Theme;
                let sub = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("").to_uppercase();
                // Map name to Theme variant.
                let theme: Option<Theme> = match sub.as_str() {
                    "DARK"             => Some(Theme::Dark),
                    "LIGHT"            => Some(Theme::Light),
                    "DRACULA"          => Some(Theme::Dracula),
                    "NORD"             => Some(Theme::Nord),
                    "SOLARIZED_LIGHT" | "SOLARIZEDLIGHT"  => Some(Theme::SolarizedLight),
                    "SOLARIZED_DARK"  | "SOLARIZEDDARK"   => Some(Theme::SolarizedDark),
                    "GRUVBOX_LIGHT"   | "GRUVBOXLIGHT"    => Some(Theme::GruvboxLight),
                    "GRUVBOX_DARK"    | "GRUVBOXDARK"     => Some(Theme::GruvboxDark),
                    "TOKYONIGHT"      | "TOKYO_NIGHT"     => Some(Theme::TokyoNight),
                    "TOKYONIGHTSTORM" | "TOKYO_NIGHT_STORM" => Some(Theme::TokyoNightStorm),
                    "TOKYONIGHTLIGHT" | "TOKYO_NIGHT_LIGHT" => Some(Theme::TokyoNightLight),
                    "KANAGAWAWAVE"    | "KANAGAWA_WAVE"   => Some(Theme::KanagawaWave),
                    "KANAGAWADRAGON"  | "KANAGAWA_DRAGON" => Some(Theme::KanagawaDragon),
                    "KANAGAWALOTUS"   | "KANAGAWA_LOTUS"  => Some(Theme::KanagawaLotus),
                    "MOONFLY"         => Some(Theme::Moonfly),
                    "NIGHTFLY"        => Some(Theme::Nightfly),
                    "OXOCARBON"       => Some(Theme::Oxocarbon),
                    "FERRA"           => Some(Theme::Ferra),
                    "" | "LIST" | "?" => {
                        self.command_line.push_output(
                            "Available themes: DARK LIGHT DRACULA NORD SOLARIZED_LIGHT SOLARIZED_DARK \
                             GRUVBOX_LIGHT GRUVBOX_DARK TOKYONIGHT TOKYONIGHTSTORM TOKYONIGHTLIGHT \
                             KANAGAWAWAVE KANAGAWADRAGON KANAGAWALOTUS MOONFLY NIGHTFLY OXOCARBON FERRA"
                        );
                        return Task::none();
                    }
                    _ => {
                        self.command_line.push_error(&format!("COLORSCHEME: unknown theme '{}'. Type COLORSCHEME LIST for options.", sub));
                        return Task::none();
                    }
                };
                if let Some(t) = theme {
                    let name = format!("{:?}", t);
                    self.command_line.push_output(&format!("Color scheme set to '{name}'."));
                    return Task::done(Message::SetTheme(t));
                }
                return Task::none();
            }

            // ── Layout Manager GUI ─────────────────────────────────────────
            "LAYOUTMANAGER"|"LAYOUTPANEL" => {
                return Task::done(Message::LayoutManagerOpen);
            }

            // ── Layout / viewport ──────────────────────────────────────────
            "MVIEW"|"MV" => {
                if self.tabs[i].scene.current_layout == "Model" {
                    self.command_line.push_error("MVIEW: switch to a paper space layout first.");
                } else {
                    use crate::modules::layout::mview::MviewCommand;
                    let new_cmd = MviewCommand::new();
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            // ── MSPACE / PSPACE ───────────────────────────────────────────
            "MS"|"MSPACE" => {
                return Task::done(Message::MspaceCommand);
            }
            "PSPACE" => {
                return Task::done(Message::PspaceCommand);
            }

            // ── VPORTS — list or create preset viewport configurations ────
            cmd if cmd == "VPORTS" || cmd.starts_with("VPORTS ") => {
                let sub = cmd.split_whitespace().nth(1).unwrap_or("").to_uppercase();
                let scene = &self.tabs[i].scene;
                if scene.current_layout == "Model" {
                    self.command_line.push_error("VPORTS: switch to a paper space layout first.");
                } else if sub.is_empty() {
                    // ── List existing viewports ──────────────────────────
                    let layout_block = scene.current_layout_block_handle_pub();
                    let viewports: Vec<_> = scene.document.entities()
                        .filter_map(|e| {
                            if let acadrust::EntityType::Viewport(vp) = e {
                                if vp.id > 1 && vp.common.owner_handle == layout_block {
                                    Some((vp.id, vp.center.clone(), vp.width, vp.height, vp.custom_scale, vp.status.is_on, vp.status.locked))
                                } else { None }
                            } else { None }
                        })
                        .collect();
                    if viewports.is_empty() {
                        self.command_line.push_info("No viewports. Use MVIEW to create one, or VPORTS 2H / 2V / 4 / SINGLE.");
                    } else {
                        self.command_line.push_output(&format!("{} viewport(s) in layout \"{}\":", viewports.len(), scene.current_layout));
                        for (id, center, w, h, scale, is_on, locked) in &viewports {
                            let state = match (is_on, locked) {
                                (true, true)  => "On, Locked",
                                (true, false) => "On",
                                (false, _)    => "Off",
                            };
                            self.command_line.push_output(&format!(
                                "  VP #{id}: {w:.1}×{h:.1} @ ({:.1},{:.1})  scale={scale:.4}  [{state}]",
                                center.x, center.y
                            ));
                        }
                    }
                } else {
                    // ── Preset viewport layout ───────────────────────────
                    // Determine paper dimensions from PlotSettings (fallback A4 landscape).
                    let layout_name = scene.current_layout.clone();
                    let (paper_w, paper_h) = {
                        use acadrust::objects::ObjectType;
                        let mut pw = 297.0_f64;
                        let mut ph = 210.0_f64;
                        for (_, obj) in &scene.document.objects {
                            if let ObjectType::PlotSettings(ps) = obj {
                                if ps.page_name == layout_name && ps.paper_width > 0.0 {
                                    pw = ps.paper_width;
                                    ph = ps.paper_height;
                                    break;
                                }
                            }
                        }
                        (pw, ph)
                    };
                    let margin = 5.0_f64; // mm margin around the usable area
                    let uw = paper_w - 2.0 * margin; // usable width
                    let uh = paper_h - 2.0 * margin; // usable height
                    // Collect rectangle specs: (cx, cz, w, h) in mm
                    let rects: Vec<(f64, f64, f64, f64)> = match sub.as_str() {
                        "2H" => {
                            // Two viewports side by side (horizontal split)
                            let vw = (uw - 2.0) / 2.0;
                            vec![
                                (margin + vw / 2.0,          margin + uh / 2.0, vw, uh),
                                (margin + vw + 2.0 + vw / 2.0, margin + uh / 2.0, vw, uh),
                            ]
                        }
                        "2V" => {
                            // Two viewports stacked (vertical split)
                            let vh = (uh - 2.0) / 2.0;
                            vec![
                                (margin + uw / 2.0, margin + vh + 2.0 + vh / 2.0, uw, vh),
                                (margin + uw / 2.0, margin + vh / 2.0,            uw, vh),
                            ]
                        }
                        "4" => {
                            // Four equal viewports (2×2 grid)
                            let vw = (uw - 2.0) / 2.0;
                            let vh = (uh - 2.0) / 2.0;
                            vec![
                                (margin + vw / 2.0,              margin + vh + 2.0 + vh / 2.0, vw, vh),
                                (margin + vw + 2.0 + vw / 2.0,  margin + vh + 2.0 + vh / 2.0, vw, vh),
                                (margin + vw / 2.0,              margin + vh / 2.0,             vw, vh),
                                (margin + vw + 2.0 + vw / 2.0,  margin + vh / 2.0,             vw, vh),
                            ]
                        }
                        "SINGLE" | "1" => {
                            // Single full-page viewport
                            vec![(margin + uw / 2.0, margin + uh / 2.0, uw, uh)]
                        }
                        _ => {
                            self.command_line.push_error(
                                "VPORTS: unknown option. Use VPORTS 2H | 2V | 4 | SINGLE"
                            );
                            vec![]
                        }
                    };
                    if !rects.is_empty() {
                        // Remove existing user viewports in this layout first.
                        let layout_block = self.tabs[i].scene.current_layout_block_handle_pub();
                        let to_erase: Vec<acadrust::Handle> = self.tabs[i].scene.document.entities()
                            .filter_map(|e| {
                                if let acadrust::EntityType::Viewport(vp) = e {
                                    if vp.id > 1 && vp.common.owner_handle == layout_block {
                                        Some(vp.common.handle)
                                    } else { None }
                                } else { None }
                            })
                            .collect();
                        self.push_undo_snapshot(i, "VPORTS");
                        self.tabs[i].scene.erase_entities(&to_erase);
                        // Create new viewports.
                        for (cx, cz, w, h) in &rects {
                            let mut vp = acadrust::entities::Viewport::new();
                            vp.center = acadrust::types::Vector3::new(*cx, 0.0, *cz);
                            vp.width  = *w;
                            vp.height = *h;
                            vp.id     = 2; // commit_entity will assign unique IDs
                            match self.tabs[i].scene.document.add_entity_to_layout(
                                acadrust::EntityType::Viewport(vp),
                                &layout_name,
                            ) {
                                Ok(handle) => {
                                    self.tabs[i].scene.auto_fit_viewport(handle);
                                }
                                Err(e) => {
                                    self.command_line.push_error(&format!("VPORTS: {e}"));
                                }
                            }
                        }
                        // Re-assign unique IDs (1 + existing max per viewport).
                        let layout_block2 = self.tabs[i].scene.current_layout_block_handle_pub();
                        let mut id_counter = 2_i16;
                        let handles: Vec<acadrust::Handle> = self.tabs[i].scene.document.entities()
                            .filter_map(|e| {
                                if let acadrust::EntityType::Viewport(vp) = e {
                                    if vp.id >= 2 && vp.common.owner_handle == layout_block2 {
                                        Some(vp.common.handle)
                                    } else { None }
                                } else { None }
                            })
                            .collect();
                        for h in handles {
                            if let Some(acadrust::EntityType::Viewport(vp)) =
                                self.tabs[i].scene.document.get_entity_mut(h)
                            {
                                vp.id = id_counter;
                                id_counter += 1;
                            }
                        }
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!(
                            "VPORTS: created {} viewport(s) [{}].", rects.len(), sub
                        ));
                    }
                }
            }

            // ── VPLAYER — per-viewport layer freeze/thaw ──────────────────
            "VPLAYER" => {
                let scene = &self.tabs[i].scene;
                if scene.current_layout == "Model" {
                    self.command_line.push_error("VPLAYER: switch to a paper space layout first.");
                } else if scene.active_viewport.is_none() {
                    self.command_line.push_error("VPLAYER: enter a viewport first (double-click or MS).");
                } else {
                    use crate::modules::layout::vplayer::VplayerCommand;
                    let vp_handle = scene.active_viewport.unwrap();
                    // Collect current frozen layer names for display.
                    let frozen_names: Vec<String> = {
                        if let Some(acadrust::EntityType::Viewport(vp)) =
                            scene.document.get_entity(vp_handle)
                        {
                            vp.frozen_layers.iter().filter_map(|h| {
                                scene.document.layers.iter().find(|l| l.handle == *h).map(|l| l.name.clone())
                            }).collect()
                        } else { vec![] }
                    };
                    if frozen_names.is_empty() {
                        self.command_line.push_info("VPLAYER: no frozen layers in active viewport.");
                    } else {
                        self.command_line.push_info(&format!(
                            "VPLAYER: frozen layers: {}",
                            frozen_names.join(", ")
                        ));
                    }
                    let new_cmd = VplayerCommand::new(vp_handle);
                    self.command_line.push_info(&new_cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(new_cmd));
                }
            }

            // ── Draw Order ────────────────────────────────────────────────
            cmd if cmd.starts_with("DRAWORDER") => {
                use acadrust::objects::{ObjectType, SortEntitiesTable};
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                let option = parts.get(1).unwrap_or(&"").to_uppercase();
                let i = self.active_tab;
                let selected: Vec<acadrust::Handle> = self.tabs[i].scene
                    .selected_entities()
                    .iter()
                    .map(|(h, _)| *h)
                    .collect();
                if selected.is_empty() {
                    self.command_line.push_error("DRAWORDER: select entities first.");
                } else {
                    // Parse relative target handle for ABOVE/UNDER.
                    let relative_target: Option<(bool, acadrust::Handle)> = match option.as_str() {
                        "A" | "ABOVE" => {
                            let h_val = parts.get(2).and_then(|s| u64::from_str_radix(s, 16).ok());
                            h_val.map(|v| (true, acadrust::Handle::new(v)))
                        }
                        "U" | "UNDER" | "BELOW" => {
                            let h_val = parts.get(2).and_then(|s| u64::from_str_radix(s, 16).ok());
                            h_val.map(|v| (false, acadrust::Handle::new(v)))
                        }
                        _ => None,
                    };
                    let to_front_opt = match option.as_str() {
                        "F" | "FRONT" => Some(true),
                        "B" | "BACK"  => Some(false),
                        _ => None,
                    };

                    if relative_target.is_some() || to_front_opt.is_some() {
                        self.push_undo_snapshot(i, "DRAWORDER");
                        let block_handle = self.tabs[i].scene.current_layout_block_handle_pub();
                        let doc = &mut self.tabs[i].scene.document;
                        let table_handle = doc.objects.iter()
                            .find_map(|(h, obj)| {
                                if let ObjectType::SortEntitiesTable(t) = obj {
                                    if t.block_owner_handle == block_handle { Some(*h) } else { None }
                                } else { None }
                            });
                        let get_or_create = |doc: &mut acadrust::CadDocument, block_handle| -> acadrust::Handle {
                            if let Some(th) = doc.objects.iter()
                                .find_map(|(h, obj)| {
                                    if let ObjectType::SortEntitiesTable(t) = obj {
                                        if t.block_owner_handle == block_handle { Some(*h) } else { None }
                                    } else { None }
                                })
                            {
                                th
                            } else {
                                let nh = acadrust::Handle::new(doc.next_handle());
                                let mut table = SortEntitiesTable::for_block(block_handle);
                                table.handle = nh;
                                doc.objects.insert(nh, ObjectType::SortEntitiesTable(table));
                                nh
                            }
                        };
                        let th = table_handle.unwrap_or_else(|| {
                            let nh = acadrust::Handle::new(doc.next_handle());
                            let mut table = SortEntitiesTable::for_block(block_handle);
                            table.handle = nh;
                            doc.objects.insert(nh, ObjectType::SortEntitiesTable(table));
                            nh
                        });
                        let _ = get_or_create; // suppress unused warning
                        if let Some(ObjectType::SortEntitiesTable(table)) = doc.objects.get_mut(&th) {
                            if let Some((above, target)) = relative_target {
                                for h in &selected {
                                    if above { table.move_above(*h, target); }
                                    else      { table.move_below(*h, target); }
                                }
                                let rel = if above { "above" } else { "below" };
                                self.command_line.push_info(&format!(
                                    "DRAWORDER: moved {} entities {} {:x}.", selected.len(), rel, target.value()
                                ));
                            } else if let Some(to_front) = to_front_opt {
                                for h in &selected {
                                    if to_front { table.bring_to_front(*h); }
                                    else        { table.send_to_back(*h); }
                                }
                                let dir = if to_front { "front" } else { "back" };
                                self.command_line.push_info(&format!(
                                    "DRAWORDER: moved {} entities to {}.", selected.len(), dir
                                ));
                            }
                        }
                        self.tabs[i].dirty = true;
                    } else {
                        self.command_line.push_info(
                            "Usage: DRAWORDER F|FRONT | B|BACK | A|ABOVE <handle> | U|UNDER <handle>"
                        );
                    }
                }
            }

            // ── LAYER management ─────────────────────────────────────────
            cmd if cmd == "LAYER" || cmd.starts_with("LAYER ") || cmd.starts_with("LA ") => {
                use acadrust::tables::Layer;
                let raw_rest = if cmd.starts_with("LAYER ") {
                    cmd.trim_start_matches("LAYER ").trim()
                } else if cmd.starts_with("LA ") {
                    cmd.trim_start_matches("LA ").trim()
                } else {
                    ""
                };
                let parts: Vec<&str> = raw_rest.split_whitespace().collect();
                let sub = parts.get(0).map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "LIST" | "?" => {
                        let info: Vec<String> = self.tabs[i].scene.document.layers.iter().map(|l| {
                            let state = if l.flags.frozen { "frozen" }
                                       else if l.flags.off { "off" }
                                       else if l.flags.locked { "locked" }
                                       else { "on" };
                            format!("{}({})", l.name, state)
                        }).collect();
                        self.command_line.push_output(&format!("Layers: {}", info.join(", ")));
                    }
                    "NEW" | "N" => {
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: LAYER NEW <name>");
                        } else if self.tabs[i].scene.document.layers.contains(&name) {
                            self.command_line.push_error(&format!("LAYER: '{}' already exists.", name));
                        } else {
                            let layer = Layer::new(&name);
                            let _ = self.tabs[i].scene.document.layers.add(layer);
                            self.push_undo_snapshot(i, "LAYER NEW");
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!("LAYER: '{}' created.", name));
                        }
                    }
                    "ON" => {
                        for name in &parts[1..] {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(name) {
                                l.flags.off = false; l.flags.frozen = false;
                            }
                        }
                        self.push_undo_snapshot(i, "LAYER ON");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output("LAYER: layers turned on.");
                    }
                    "OFF" => {
                        for name in &parts[1..] {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(name) {
                                l.flags.off = true;
                            }
                        }
                        self.push_undo_snapshot(i, "LAYER OFF");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output("LAYER: layers turned off.");
                    }
                    "FREEZE" | "FR" => {
                        for name in &parts[1..] {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(name) {
                                l.flags.frozen = true;
                            }
                        }
                        self.push_undo_snapshot(i, "LAYER FREEZE");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output("LAYER: layers frozen.");
                    }
                    "THAW" | "TH" => {
                        for name in &parts[1..] {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(name) {
                                l.flags.frozen = false;
                            }
                        }
                        self.push_undo_snapshot(i, "LAYER THAW");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output("LAYER: layers thawed.");
                    }
                    "LOCK" | "LO" => {
                        for name in &parts[1..] {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(name) {
                                l.flags.locked = true;
                            }
                        }
                        self.push_undo_snapshot(i, "LAYER LOCK");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output("LAYER: layers locked.");
                    }
                    "UNLOCK" | "UL" => {
                        for name in &parts[1..] {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(name) {
                                l.flags.locked = false;
                            }
                        }
                        self.push_undo_snapshot(i, "LAYER UNLOCK");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output("LAYER: layers unlocked.");
                    }
                    "COLOR" | "C" => {
                        // LAYER COLOR <name> <aci_index>
                        let layer_name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        let color_str = parts.get(2).map(|s| s.trim()).unwrap_or("");
                        if let Ok(idx) = color_str.parse::<i16>() {
                            if let Some(l) = self.tabs[i].scene.document.layers.get_mut(&layer_name) {
                                l.color = acadrust::types::Color::from_index(idx);
                                self.push_undo_snapshot(i, "LAYER COLOR");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("LAYER: '{}' color set to ACI {}.", layer_name, idx));
                            } else {
                                self.command_line.push_error(&format!("LAYER: '{}' not found.", layer_name));
                            }
                        } else {
                            self.command_line.push_error("Usage: LAYER COLOR <name> <aci_index>");
                        }
                    }
                    "SET" | "S" | "CURRENT" => {
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        if self.tabs[i].scene.document.layers.contains(&name) {
                            self.tabs[i].layers.current_layer = name.clone();
                            self.command_line.push_output(&format!("LAYER: current layer set to '{}'.", name));
                        } else {
                            self.command_line.push_error(&format!("LAYER: '{}' not found.", name));
                        }
                    }
                    _ => {
                        self.command_line.push_info(
                            "Usage: LAYER LIST | NEW <name> | ON/OFF/FREEZE/THAW/LOCK/UNLOCK <name> | COLOR <name> <aci> | SET <name>"
                        );
                    }
                }
            }

            // ── UCS management ───────────────────────────────────────────
            cmd if cmd == "UCS" || cmd.starts_with("UCS ") => {
                use acadrust::tables::Ucs;
                use acadrust::types::Vector3;
                use super::helpers::{ucs_to_wcs, ucs_z_axis, ucs_rotated_z};
                let parts: Vec<&str> = cmd.splitn(4, ' ').collect();
                let sub = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "LIST" | "?" => {
                        let active_name = self.tabs[i].active_ucs.as_ref()
                            .map(|u| u.name.clone())
                            .unwrap_or_else(|| "WCS".into());
                        let names: Vec<String> = self.tabs[i].scene.document
                            .ucss.iter().map(|u| u.name.clone()).collect();
                        if names.is_empty() {
                            self.command_line.push_output(&format!(
                                "Active UCS: {}  |  No named UCSs defined.", active_name
                            ));
                        } else {
                            self.command_line.push_output(&format!(
                                "Active UCS: {}  |  Named: {}", active_name, names.join(", ")
                            ));
                        }
                    }
                    "SAVE" | "S" => {
                        let name = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: UCS SAVE <name>");
                        } else {
                            // Save the current active UCS under this name.
                            let ucs = match &self.tabs[i].active_ucs {
                                Some(u) => {
                                    let mut saved = u.clone();
                                    saved.name = name.clone();
                                    saved
                                }
                                None => Ucs::new(&name), // save WCS (identity)
                            };
                            self.tabs[i].scene.document.ucss.add_or_replace(ucs);
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!("UCS '{}' saved.", name));
                        }
                    }
                    "DELETE" | "DEL" | "D" => {
                        let name = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: UCS DELETE <name>");
                        } else if self.tabs[i].scene.document.ucss.remove(&name).is_some() {
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!("UCS '{}' deleted.", name));
                        } else {
                            self.command_line.push_error(&format!("UCS '{}' not found.", name));
                        }
                    }
                    "W" | "WORLD" => {
                        self.tabs[i].active_ucs = None;
                        self.command_line.push_output("UCS reset to World Coordinate System.");
                    }
                    // UCS ORIGIN x,y,z  — shift the active UCS origin, keep axes
                    "ORIGIN" | "O" => {
                        let coord_str = parts.get(2).copied().unwrap_or("");
                        if let Some(pt) = super::helpers::parse_coord(coord_str) {
                            // `pt` is in current UCS space; convert to WCS
                            let wcs_origin = if let Some(ref ucs) = self.tabs[i].active_ucs {
                                ucs_to_wcs(pt, ucs)
                            } else {
                                pt
                            };
                            let ucs = self.tabs[i].active_ucs.get_or_insert_with(|| Ucs::new("*ACTIVE*"));
                            ucs.origin = Vector3::new(
                                wcs_origin.x as f64, wcs_origin.y as f64, wcs_origin.z as f64,
                            );
                            self.command_line.push_output(&format!(
                                "UCS origin set to ({:.4}, {:.4}, {:.4}).",
                                wcs_origin.x, wcs_origin.y, wcs_origin.z
                            ));
                        } else {
                            self.command_line.push_error("Usage: UCS ORIGIN x,y,z");
                        }
                    }
                    // UCS Z angle  — rotate active UCS around its Z axis by degrees
                    "Z" => {
                        let deg: Option<f32> = parts.get(2).and_then(|s| s.trim().parse().ok());
                        if let Some(angle_deg) = deg {
                            let rad = angle_deg.to_radians();
                            let current = self.tabs[i].active_ucs.as_ref();
                            let origin = current.map(|u| {
                                glam::Vec3::new(
                                    u.origin.x as f32, u.origin.y as f32, u.origin.z as f32,
                                )
                            }).unwrap_or(glam::Vec3::ZERO);
                            let mut new_ucs = ucs_rotated_z(origin, rad);
                            // If already had axes, compose rotation on top
                            if let Some(ref ucs) = self.tabs[i].active_ucs {
                                let old_x = glam::Vec3::new(
                                    ucs.x_axis.x as f32, ucs.x_axis.y as f32, ucs.x_axis.z as f32,
                                );
                                let old_y = glam::Vec3::new(
                                    ucs.y_axis.x as f32, ucs.y_axis.y as f32, ucs.y_axis.z as f32,
                                );
                                let z_ax = ucs_z_axis(ucs);
                                let rot = glam::Quat::from_axis_angle(z_ax, rad);
                                let nx = rot * old_x;
                                let ny = rot * old_y;
                                new_ucs.x_axis = Vector3::new(
                                    nx.x as f64, nx.y as f64, nx.z as f64,
                                );
                                new_ucs.y_axis = Vector3::new(
                                    ny.x as f64, ny.y as f64, ny.z as f64,
                                );
                            }
                            self.tabs[i].active_ucs = Some(new_ucs);
                            self.command_line.push_output(&format!(
                                "UCS rotated {:.2}° around Z.", angle_deg
                            ));
                        } else {
                            self.command_line.push_error("Usage: UCS Z <angle_degrees>");
                        }
                    }
                    // UCS X angle  — rotate around current UCS X axis
                    "X" => {
                        let deg: Option<f32> = parts.get(2).and_then(|s| s.trim().parse().ok());
                        if let Some(angle_deg) = deg {
                            let rad = angle_deg.to_radians();
                            let ucs = self.tabs[i].active_ucs.get_or_insert_with(|| Ucs::new("*ACTIVE*"));
                            let x_ax = glam::Vec3::new(
                                ucs.x_axis.x as f32, ucs.x_axis.y as f32, ucs.x_axis.z as f32,
                            );
                            let old_y = glam::Vec3::new(
                                ucs.y_axis.x as f32, ucs.y_axis.y as f32, ucs.y_axis.z as f32,
                            );
                            let rot = glam::Quat::from_axis_angle(x_ax, rad);
                            let ny = rot * old_y;
                            ucs.y_axis = Vector3::new(ny.x as f64, ny.y as f64, ny.z as f64);
                            self.command_line.push_output(&format!(
                                "UCS rotated {:.2}° around X.", angle_deg
                            ));
                        } else {
                            self.command_line.push_error("Usage: UCS X <angle_degrees>");
                        }
                    }
                    // UCS Y angle  — rotate around current UCS Y axis
                    "Y" => {
                        let deg: Option<f32> = parts.get(2).and_then(|s| s.trim().parse().ok());
                        if let Some(angle_deg) = deg {
                            let rad = angle_deg.to_radians();
                            let ucs = self.tabs[i].active_ucs.get_or_insert_with(|| Ucs::new("*ACTIVE*"));
                            let y_ax = glam::Vec3::new(
                                ucs.y_axis.x as f32, ucs.y_axis.y as f32, ucs.y_axis.z as f32,
                            );
                            let old_x = glam::Vec3::new(
                                ucs.x_axis.x as f32, ucs.x_axis.y as f32, ucs.x_axis.z as f32,
                            );
                            let rot = glam::Quat::from_axis_angle(y_ax, rad);
                            let nx = rot * old_x;
                            ucs.x_axis = Vector3::new(nx.x as f64, nx.y as f64, nx.z as f64);
                            self.command_line.push_output(&format!(
                                "UCS rotated {:.2}° around Y.", angle_deg
                            ));
                        } else {
                            self.command_line.push_error("Usage: UCS Y <angle_degrees>");
                        }
                    }
                    _ => {
                        // UCS <name> — activate a named UCS
                        let name = sub.clone();
                        if let Some(named) = self.tabs[i].scene.document.ucss.get(&name).cloned() {
                            self.tabs[i].active_ucs = Some(named);
                            self.command_line.push_output(&format!("UCS '{}' activated.", name));
                        } else {
                            self.command_line.push_error(&format!(
                                "UCS '{}' not found.  Usage: UCS LIST | SAVE <name> | DELETE <name> | W | ORIGIN x,y,z | X/Y/Z <angle>",
                                name
                            ));
                        }
                    }
                }
            }

            // ── Named Views (VIEW command) ────────────────────────────────
            cmd if cmd == "VIEW" || cmd.starts_with("VIEW ") => {
                let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
                let sub = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "LIST" | "?" => {
                        let views: Vec<String> = self.tabs[i].scene.document
                            .views.iter().map(|v| v.name.clone()).collect();
                        if views.is_empty() {
                            self.command_line.push_output("No named views saved.");
                        } else {
                            self.command_line.push_output(&format!(
                                "Named views: {}", views.join(", ")
                            ));
                        }
                    }
                    "SAVE" | "S" => {
                        let name = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: VIEW SAVE <name>");
                        } else {
                            let new_view = self.tabs[i].scene.current_as_named_view(&name);
                            self.tabs[i].scene.document.views.add_or_replace(new_view);
                            self.command_line.push_output(&format!("View '{}' saved.", name));
                        }
                    }
                    "DELETE" | "DEL" | "D" => {
                        let name = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: VIEW DELETE <name>");
                        } else {
                            if self.tabs[i].scene.document.views.remove(&name).is_some() {
                                self.command_line.push_output(&format!("View '{}' deleted.", name));
                            } else {
                                self.command_line.push_error(&format!("View '{}' not found.", name));
                            }
                        }
                    }
                    "RESTORE" | "R" => {
                        let name = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: VIEW RESTORE <name>");
                        } else {
                            let found = self.tabs[i].scene.document.views.get(&name).cloned();
                            if let Some(v) = found {
                                self.tabs[i].scene.restore_named_view(&v);
                                self.command_line.push_output(&format!("View '{}' restored.", v.name));
                            } else {
                                self.command_line.push_error(&format!("View '{}' not found.", name));
                            }
                        }
                    }
                    // VIEW <name> shortcut for restore
                    _ => {
                        let name = sub.clone();
                        let found = self.tabs[i].scene.document.views.get(&name).cloned();
                        if let Some(v) = found {
                            self.tabs[i].scene.restore_named_view(&v);
                            self.command_line.push_output(&format!("View '{}' restored.", v.name));
                        } else {
                            self.command_line.push_error(
                                "Usage: VIEW LIST | VIEW SAVE <name> | VIEW RESTORE <name> | VIEW DELETE <name>"
                            );
                        }
                    }
                }
            }

            // ── DimStyle management ───────────────────────────────────────
            // TABLESTYLE — Table Style Manager.
            cmd if cmd == "TABLESTYLE" || cmd == "TS" || cmd.starts_with("TABLESTYLE ") => {
                use acadrust::objects::{TableStyle, ObjectType};
                let raw_rest = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                let parts: Vec<&str> = raw_rest.split_whitespace().collect();
                let sub = parts.first().map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "DIALOG" | "UI" => {
                        return Task::done(Message::TableStyleDialogOpen);
                    }
                    "LIST" | "?" => {
                        let doc = &self.tabs[i].scene.document;
                        let styles: Vec<String> = doc.objects.values()
                            .filter_map(|o| if let ObjectType::TableStyle(s) = o { Some(s) } else { None })
                            .map(|s| format!("{}  (h_margin:{:.2} v_margin:{:.2})", s.name, s.horizontal_margin, s.vertical_margin))
                            .collect();
                        if styles.is_empty() {
                            self.command_line.push_output("No table styles.");
                        } else {
                            self.command_line.push_output(&format!("TableStyles:\n  {}", styles.join("\n  ")));
                        }
                    }
                    "NEW" | "N" => {
                        let name = parts.get(1).copied().unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: TABLESTYLE NEW <name>");
                        } else {
                            let doc = &self.tabs[i].scene.document;
                            let exists = doc.objects.values().any(|o| {
                                matches!(o, ObjectType::TableStyle(s) if s.name.eq_ignore_ascii_case(&name))
                            });
                            if exists {
                                self.command_line.push_error(&format!("TABLESTYLE: '{}' already exists.", name));
                            } else {
                                self.push_undo_snapshot(i, "TABLESTYLE NEW");
                                let mut style = TableStyle::standard();
                                style.name = name.clone();
                                let nh = acadrust::Handle::new(self.tabs[i].scene.document.next_handle());
                                style.handle = nh;
                                self.tabs[i].scene.document.objects.insert(nh, ObjectType::TableStyle(style));
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("TABLESTYLE: '{}' created.", name));
                            }
                        }
                    }
                    _ => {
                        self.command_line.push_error(
                            "Usage: TABLESTYLE [LIST|NEW <name>]"
                        );
                    }
                }
            }

            // MLSTYLE — Multiline Style Manager.
            // Usage:
            //   MLSTYLE                — open dialog
            //   MLSTYLE LIST / ?       — list all multiline styles
            //   MLSTYLE NEW <name>     — create a new style
            //   MLSTYLE SET <name>     — set current multiline style
            //   MLSTYLE DEL <name>     — delete a style (not Standard)
            cmd if cmd == "MLSTYLE" || cmd.starts_with("MLSTYLE ") => {
                use acadrust::objects::{MLineStyle, ObjectType};
                let raw_rest = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                let parts: Vec<&str> = raw_rest.split_whitespace().collect();
                let sub = parts.first().map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "DIALOG" | "UI" => {
                        return Task::done(Message::MlStyleDialogOpen);
                    }
                    "LIST" | "?" => {
                        let doc = &self.tabs[i].scene.document;
                        let current = &doc.header.multiline_style;
                        let styles: Vec<String> = doc.objects.values()
                            .filter_map(|o| if let ObjectType::MLineStyle(s) = o { Some(s) } else { None })
                            .map(|s| {
                                let cur = if &s.name == current { " (current)" } else { "" };
                                format!("{}  [{}]{}",
                                    s.name,
                                    s.elements.len(),
                                    cur)
                            })
                            .collect();
                        if styles.is_empty() {
                            self.command_line.push_output("No multiline styles.");
                        } else {
                            self.command_line.push_output(&format!("MLineStyles:\n  {}", styles.join("\n  ")));
                        }
                    }
                    "NEW" | "N" => {
                        let name = parts.get(1).copied().unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: MLSTYLE NEW <name>");
                        } else {
                            let doc = &self.tabs[i].scene.document;
                            let exists = doc.objects.values().any(|o| {
                                matches!(o, ObjectType::MLineStyle(s) if s.name.eq_ignore_ascii_case(&name))
                            });
                            if exists {
                                self.command_line.push_error(&format!("MLSTYLE: '{}' already exists.", name));
                            } else {
                                self.push_undo_snapshot(i, "MLSTYLE NEW");
                                let mut style = MLineStyle::standard();
                                style.name = name.clone();
                                let nh = acadrust::Handle::new(self.tabs[i].scene.document.next_handle());
                                style.handle = nh;
                                self.tabs[i].scene.document.objects.insert(
                                    nh, ObjectType::MLineStyle(style)
                                );
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("MLSTYLE: '{}' created.", name));
                            }
                        }
                    }
                    "SET" | "S" => {
                        let name = parts.get(1).copied().unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: MLSTYLE SET <name>");
                        } else {
                            let doc = &self.tabs[i].scene.document;
                            let exists = doc.objects.values().any(|o| {
                                matches!(o, ObjectType::MLineStyle(s) if s.name.eq_ignore_ascii_case(&name))
                            });
                            if exists {
                                self.push_undo_snapshot(i, "MLSTYLE SET");
                                self.tabs[i].scene.document.header.multiline_style = name.clone();
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("MLSTYLE: current style set to '{}'.", name));
                            } else {
                                self.command_line.push_error(&format!("MLSTYLE: '{}' not found.", name));
                            }
                        }
                    }
                    "DEL" | "DELETE" => {
                        let name = parts.get(1).copied().unwrap_or("").to_string();
                        if name.is_empty() || name.eq_ignore_ascii_case("Standard") {
                            self.command_line.push_error("Cannot delete the Standard style.");
                        } else {
                            let doc = &self.tabs[i].scene.document;
                            let handle = doc.objects.iter()
                                .find_map(|(&h, o)| {
                                    if let ObjectType::MLineStyle(s) = o {
                                        if s.name.eq_ignore_ascii_case(&name) { Some(h) } else { None }
                                    } else { None }
                                });
                            if let Some(h) = handle {
                                self.push_undo_snapshot(i, "MLSTYLE DEL");
                                self.tabs[i].scene.document.objects.remove(&h);
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("MLSTYLE: '{}' deleted.", name));
                            } else {
                                self.command_line.push_error(&format!("MLSTYLE: '{}' not found.", name));
                            }
                        }
                    }
                    _ => {
                        self.command_line.push_error(
                            "Usage: MLSTYLE [LIST|NEW <name>|SET <name>|DEL <name>]"
                        );
                    }
                }
            }

            cmd if cmd == "DIMSTYLE" || cmd == "DDIM" || cmd.starts_with("DIMSTYLE ") || cmd.starts_with("DDIM ") => {
                use acadrust::tables::DimStyle;
                let raw_rest = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                let parts: Vec<&str> = raw_rest.split_whitespace().collect();
                let sub = parts.get(0).map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    // No sub-command or "DIALOG" → open the DimStyle Manager dialog
                    "" | "DIALOG" | "UI" => {
                        return Task::done(Message::DimStyleDialogOpen);
                    }
                    "LIST" | "?" => {
                        let styles: Vec<String> = self.tabs[i].scene.document
                            .dim_styles.iter()
                            .map(|s| format!("{}(txt:{:.2} asz:{:.2})", s.name, s.dimtxt, s.dimasz))
                            .collect();
                        if styles.is_empty() {
                            self.command_line.push_output("No dim styles defined.");
                        } else {
                            self.command_line.push_output(&format!("DimStyles: {}", styles.join(", ")));
                        }
                    }
                    "NEW" | "N" => {
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: DIMSTYLE NEW <name>");
                        } else if self.tabs[i].scene.document.dim_styles.contains(&name) {
                            self.command_line.push_error(&format!("DIMSTYLE: '{}' already exists.", name));
                        } else {
                            let style = DimStyle::new(&name);
                            let _ = self.tabs[i].scene.document.dim_styles.add(style);
                            self.push_undo_snapshot(i, "DIMSTYLE NEW");
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!("DIMSTYLE: '{}' created.", name));
                        }
                    }
                    "SET" | "S" => {
                        // DIMSTYLE SET <name> <property> <value>
                        // e.g. DIMSTYLE SET Standard dimtxt 2.5
                        let style_name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        let prop = parts.get(2).map(|s| s.to_lowercase()).unwrap_or_default();
                        let val_str = parts.get(3).map(|s| s.trim()).unwrap_or("");
                        if let Ok(val) = val_str.parse::<f64>() {
                            if let Some(ds) = self.tabs[i].scene.document.dim_styles.get_mut(&style_name) {
                                match prop.as_str() {
                                    "dimtxt"    => { ds.dimtxt   = val; }
                                    "dimasz"    => { ds.dimasz   = val; }
                                    "dimdli"    => { ds.dimdli   = val; }
                                    "dimexo"    => { ds.dimexo   = val; }
                                    "dimexe"    => { ds.dimexe   = val; }
                                    "dimgap"    => { ds.dimgap   = val; }
                                    "dimscale"  => { ds.dimscale = val; }
                                    "dimlfac"   => { ds.dimlfac  = val; }
                                    "dimdle"    => { ds.dimdle   = val; }
                                    "dimtvp"    => { ds.dimtvp   = val; }
                                    "dimcen"    => { ds.dimcen   = val; }
                                    "dimtsz"    => { ds.dimtsz   = val; }
                                    "dimfxl"    => { ds.dimfxl   = val; }
                                    _ => {
                                        self.command_line.push_error(&format!(
                                            "DIMSTYLE: unknown property '{}'. Try: dimtxt dimasz dimdli dimexo dimexe dimgap dimscale dimlfac dimdle dimcen dimtsz", prop
                                        ));
                                        return Task::none();
                                    }
                                }
                                self.push_undo_snapshot(i, "DIMSTYLE SET");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("DIMSTYLE: '{style_name}'.{prop} = {val:.3}"));
                            } else {
                                self.command_line.push_error(&format!("DIMSTYLE: '{}' not found.", style_name));
                            }
                        } else {
                            self.command_line.push_error("Usage: DIMSTYLE SET <name> <property> <value>");
                        }
                    }
                    _ => {
                        self.command_line.push_info(
                            "Usage: DIMSTYLE LIST | NEW <name> | SET <name> <prop> <val>"
                        );
                    }
                }
            }

            // ── MLeader Style management ──────────────────────────────────
            cmd if cmd == "MLEADERSTYLE" || cmd.starts_with("MLEADERSTYLE ") => {
                use acadrust::objects::{ObjectType, MultiLeaderStyle};
                let raw_rest = cmd.trim_start_matches("MLEADERSTYLE").trim();
                let parts: Vec<&str> = raw_rest.split_whitespace().collect();
                let sub = parts.first().map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "LIST" | "?" => {
                        let styles: Vec<String> = self.tabs[i].scene.document
                            .objects.values()
                            .filter_map(|o| if let ObjectType::MultiLeaderStyle(s) = o { Some(format!("{}(txt:{:.2} asz:{:.2})", s.name, s.text_height, s.arrowhead_size)) } else { None })
                            .collect();
                        let current = &self.tabs[i].active_mleader_style;
                        if styles.is_empty() {
                            self.command_line.push_output(&format!("MLeader styles: (none)  active: {current}"));
                        } else {
                            self.command_line.push_output(&format!("MLeader styles: {}  active: {current}", styles.join(", ")));
                        }
                    }
                    "NEW" | "N" => {
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error("Usage: MLEADERSTYLE NEW <name>");
                        } else {
                            let already_exists = self.tabs[i].scene.document.objects.values()
                                .any(|o| matches!(o, ObjectType::MultiLeaderStyle(s) if s.name == name));
                            if already_exists {
                                self.command_line.push_error(&format!("MLEADERSTYLE: '{}' already exists.", name));
                            } else {
                                let handle = self.tabs[i].scene.document.allocate_handle();
                                let mut style = MultiLeaderStyle::new(&name);
                                style.handle = handle;
                                self.tabs[i].scene.document.objects.insert(handle, ObjectType::MultiLeaderStyle(style));
                                self.push_undo_snapshot(i, "MLEADERSTYLE NEW");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("MLEADERSTYLE: '{}' created.", name));
                            }
                        }
                    }
                    "SET" | "S" => {
                        // MLEADERSTYLE SET <name> <property> <value>
                        // Properties: text_height arrowhead_size landing_distance landing_gap
                        let style_name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        let prop = parts.get(2).map(|s| s.to_lowercase()).unwrap_or_default();
                        let val_str = parts.get(3).map(|s| s.trim()).unwrap_or("");
                        if let Ok(val) = val_str.parse::<f64>() {
                            let style_entry = self.tabs[i].scene.document.objects.values_mut()
                                .find_map(|o| if let ObjectType::MultiLeaderStyle(s) = o { if s.name == style_name { Some(s) } else { None } } else { None });
                            if let Some(s) = style_entry {
                                match prop.as_str() {
                                    "text_height" | "textheight" | "txth" => { s.text_height = val; }
                                    "arrowhead_size" | "arrowsize" | "asz" => { s.arrowhead_size = val; }
                                    "landing_distance" | "landing" | "dogleg" => { s.landing_distance = val; }
                                    "landing_gap" | "gap" => { s.landing_gap = val; }
                                    _ => {
                                        self.command_line.push_error(&format!(
                                            "MLEADERSTYLE: unknown property '{}'. Try: text_height arrowhead_size landing_distance landing_gap", prop
                                        ));
                                        return Task::none();
                                    }
                                }
                                self.push_undo_snapshot(i, "MLEADERSTYLE SET");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("MLEADERSTYLE: '{style_name}'.{prop} = {val:.3}"));
                            } else {
                                self.command_line.push_error(&format!("MLEADERSTYLE: '{}' not found.", style_name));
                            }
                        } else {
                            self.command_line.push_error("Usage: MLEADERSTYLE SET <name> <property> <value>");
                        }
                    }
                    "CURRENT" | "C" | "ACTIVE" => {
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_output(&format!("Current MLeader style: {}", self.tabs[i].active_mleader_style));
                        } else {
                            let exists = name == "Standard" || self.tabs[i].scene.document.objects.values()
                                .any(|o| matches!(o, ObjectType::MultiLeaderStyle(s) if s.name == name));
                            if exists {
                                self.tabs[i].active_mleader_style = name.clone();
                                self.command_line.push_output(&format!("MLEADERSTYLE: current style set to '{name}'."));
                            } else {
                                self.command_line.push_error(&format!("MLEADERSTYLE: '{}' not found.", name));
                            }
                        }
                    }
                    _ => {
                        self.command_line.push_info(
                            "Usage: MLEADERSTYLE LIST | NEW <name> | SET <name> <prop> <val> | CURRENT [<name>]"
                        );
                    }
                }
            }

            // ── TextStyle / Style management ──────────────────────────────
            cmd if cmd == "STYLE" || cmd == "TEXTSTYLE" || cmd.starts_with("STYLE ") || cmd.starts_with("TEXTSTYLE ") => {
                let (prefix, rest) = if cmd.starts_with("TEXTSTYLE") {
                    ("TEXTSTYLE", cmd.trim_start_matches("TEXTSTYLE").trim())
                } else {
                    ("STYLE", cmd.trim_start_matches("STYLE").trim())
                };
                let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                let sub = parts.get(0).map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "DIALOG" | "UI" => {
                        return Task::done(Message::TextStyleDialogOpen);
                    }
                    "" | "LIST" | "?" => {
                        let styles: Vec<String> = self.tabs[i].scene.document
                            .text_styles.iter()
                            .map(|s| format!("{} (font: {}, w: {:.2}, oblique: {:.1}°)",
                                s.name, s.font_file, s.width_factor, s.oblique_angle.to_degrees()))
                            .collect();
                        if styles.is_empty() {
                            self.command_line.push_output("No text styles defined.");
                        } else {
                            self.command_line.push_output(&format!("Text styles: {}", styles.join(" | ")));
                        }
                    }
                    "SET" | "S" => {
                        // STYLE SET <name> — set active text style (for future text commands)
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("");
                        if self.tabs[i].scene.document.text_styles.get(name).is_some() {
                            self.command_line.push_output(&format!("{prefix}: active style set to '{name}'."));
                        } else {
                            self.command_line.push_error(&format!("{prefix}: style '{name}' not found."));
                        }
                    }
                    "NEW" | "N" => {
                        let name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        if name.is_empty() {
                            self.command_line.push_error(&format!("Usage: {prefix} NEW <name>"));
                        } else if self.tabs[i].scene.document.text_styles.contains(&name) {
                            self.command_line.push_error(&format!("{prefix}: style '{name}' already exists."));
                        } else {
                            let style = acadrust::tables::TextStyle::new(&name);
                            let _ = self.tabs[i].scene.document.text_styles.add(style);
                            self.push_undo_snapshot(i, "STYLE NEW");
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!("{prefix}: style '{name}' created."));
                        }
                    }
                    "FONT" | "F" => {
                        // STYLE FONT <name> <font_file>
                        let style_name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        let font = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                        if style_name.is_empty() || font.is_empty() {
                            self.command_line.push_error(&format!("Usage: {prefix} FONT <style> <font_file>"));
                        } else if let Some(s) = self.tabs[i].scene.document.text_styles.get_mut(&style_name) {
                            s.font_file = font.clone();
                            self.push_undo_snapshot(i, "STYLE FONT");
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!("{prefix}: '{style_name}' font set to '{font}'."));
                        } else {
                            self.command_line.push_error(&format!("{prefix}: style '{style_name}' not found."));
                        }
                    }
                    "WIDTH" | "W" => {
                        // STYLE WIDTH <name> <factor>
                        let style_name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        let factor_str = parts.get(2).map(|s| s.trim()).unwrap_or("");
                        if let Ok(factor) = factor_str.parse::<f64>() {
                            if let Some(s) = self.tabs[i].scene.document.text_styles.get_mut(&style_name) {
                                s.width_factor = factor;
                                self.push_undo_snapshot(i, "STYLE WIDTH");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("{prefix}: '{style_name}' width factor set to {factor:.3}."));
                            } else {
                                self.command_line.push_error(&format!("{prefix}: style '{style_name}' not found."));
                            }
                        } else {
                            self.command_line.push_error(&format!("Usage: {prefix} WIDTH <style> <factor>"));
                        }
                    }
                    "OBLIQUE" => {
                        // STYLE OBLIQUE <name> <angle_degrees>
                        let style_name = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                        let angle_str = parts.get(2).map(|s| s.trim()).unwrap_or("");
                        if let Ok(deg) = angle_str.parse::<f64>() {
                            if let Some(s) = self.tabs[i].scene.document.text_styles.get_mut(&style_name) {
                                s.oblique_angle = deg.to_radians();
                                self.push_undo_snapshot(i, "STYLE OBLIQUE");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("{prefix}: '{style_name}' oblique angle set to {deg:.1}°."));
                            } else {
                                self.command_line.push_error(&format!("{prefix}: style '{style_name}' not found."));
                            }
                        } else {
                            self.command_line.push_error(&format!("Usage: {prefix} OBLIQUE <style> <angle_degrees>"));
                        }
                    }
                    _ => {
                        self.command_line.push_info(&format!(
                            "Usage: {prefix} LIST | NEW <name> | FONT <style> <file> | WIDTH <style> <factor> | OBLIQUE <style> <angle>"
                        ));
                    }
                }
            }

            // ── LINETYPE management ───────────────────────────────────────
            cmd if cmd == "LINETYPE" || cmd == "LT" || cmd.starts_with("LINETYPE ") || cmd.starts_with("LT ") => {
                let raw_rest = cmd.split_once(' ').map(|(_, r)| r.trim()).unwrap_or("");
                let parts: Vec<&str> = raw_rest.split_whitespace().collect();
                let sub = parts.get(0).map(|s| s.to_uppercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "LIST" | "?" => {
                        let ltypes: Vec<String> = self.tabs[i].scene.document
                            .line_types.iter()
                            .map(|lt| format!("{} ({})", lt.name, lt.description))
                            .collect();
                        if ltypes.is_empty() {
                            self.command_line.push_output("No linetypes defined.");
                        } else {
                            self.command_line.push_output(&format!("Linetypes: {}", ltypes.join(", ")));
                        }
                    }
                    _ => {
                        self.command_line.push_info("Usage: LINETYPE LIST");
                    }
                }
            }

            // ── PURGE unused definitions ──────────────────────────────────
            cmd if cmd == "PURGE" || cmd.starts_with("PURGE ") => {
                let sub = cmd.split_whitespace().nth(1).unwrap_or("ALL").to_uppercase();
                let all = sub == "ALL" || sub.is_empty();

                // Collect names in use (immutable borrows — done in their own scope)
                let used_layers: std::collections::HashSet<String> = self.tabs[i].scene.document.entities()
                    .filter_map(|e| {
                        let name = &e.common().layer;
                        if name.is_empty() { None } else { Some(name.clone()) }
                    }).collect();
                let used_text_styles: std::collections::HashSet<String> = self.tabs[i].scene.document.entities()
                    .filter_map(|e| match e {
                        acadrust::EntityType::Text(t) => Some(t.style.clone()),
                        acadrust::EntityType::MText(t) => Some(t.style.clone()),
                        _ => None,
                    })
                    .filter(|s| !s.is_empty())
                    .collect();
                let used_linetypes: std::collections::HashSet<String> = self.tabs[i].scene.document.entities()
                    .filter_map(|e| {
                        let lt = &e.common().linetype;
                        if lt.is_empty() || lt == "ByLayer" || lt == "ByBlock" { None } else { Some(lt.clone()) }
                    }).collect();

                // Build removal lists (still immutable)
                let layer_remove: Vec<String> = if all || sub == "LAYERS" {
                    self.tabs[i].scene.document.layers.iter()
                        .filter(|l| l.name != "0" && !used_layers.contains(&l.name))
                        .map(|l| l.name.clone()).collect()
                } else { vec![] };
                let style_remove: Vec<String> = if all || sub == "TEXTSTYLES" || sub == "STYLES" {
                    self.tabs[i].scene.document.text_styles.iter()
                        .filter(|s| s.name != "Standard" && !used_text_styles.contains(&s.name))
                        .map(|s| s.name.clone()).collect()
                } else { vec![] };
                let lt_remove: Vec<String> = if all || sub == "LINETYPES" || sub == "LT" {
                    let standard = ["Continuous", "ByLayer", "ByBlock"];
                    self.tabs[i].scene.document.line_types.iter()
                        .filter(|lt| !standard.iter().any(|s| s.eq_ignore_ascii_case(&lt.name))
                            && !used_linetypes.contains(&lt.name))
                        .map(|lt| lt.name.clone()).collect()
                } else { vec![] };

                // Apply removals (mutable)
                let purged = layer_remove.len() + style_remove.len() + lt_remove.len();
                for name in &layer_remove { self.tabs[i].scene.document.layers.remove(name); }
                for name in &style_remove { self.tabs[i].scene.document.text_styles.remove(name); }
                for name in &lt_remove { self.tabs[i].scene.document.line_types.remove(name); }

                if purged > 0 {
                    self.push_undo_snapshot(i, "PURGE");
                    self.tabs[i].dirty = true;
                    self.command_line.push_output(&format!("PURGE: {} definition(s) removed.", purged));
                } else {
                    self.command_line.push_output("PURGE: nothing to purge.");
                }
            }

            // ── CHPROP — change entity properties from command line ───────
            cmd if cmd == "CHPROP" || cmd.starts_with("CHPROP ") => {
                // Usage: CHPROP <property> <value>
                // Applies to currently selected entities.
                // Properties: LAYER, COLOR, LINETYPE, LTSCALE
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                let prop = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
                let value = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();

                if prop.is_empty() {
                    self.command_line.push_info(
                        "Usage: CHPROP <prop> <val>  (props: LAYER COLOR LINETYPE LTSCALE)"
                    );
                } else {
                    let handles: Vec<_> = self.tabs[i].scene.selected_entities()
                        .into_iter().map(|(h, _)| h).collect();
                    if handles.is_empty() {
                        self.command_line.push_error("CHPROP: no entities selected.");
                    } else {
                        // Validate value early to give clear errors
                        let color_val: Option<acadrust::types::Color> = if prop == "COLOR" {
                            value.parse::<i16>().ok().map(acadrust::types::Color::from_index)
                        } else { None };
                        let ltscale_val: Option<f64> = if prop == "LTSCALE" {
                            value.parse().ok()
                        } else { None };
                        let transparency_val: Option<acadrust::types::Transparency> = if prop == "TRANSPARENCY" {
                            value.parse::<f64>().ok().map(acadrust::types::Transparency::from_percent)
                        } else { None };

                        if (prop == "COLOR" && color_val.is_none())
                            || (prop == "LTSCALE" && ltscale_val.is_none())
                            || (prop == "TRANSPARENCY" && transparency_val.is_none())
                        {
                            self.command_line.push_error(&format!("CHPROP: invalid value '{}' for {}.", value, prop));
                        } else {
                            let mut changed = 0usize;
                            for handle in &handles {
                                if let Some(entity) = self.tabs[i].scene.document.get_entity_mut(*handle) {
                                    let common = entity.common_mut();
                                    match prop.as_str() {
                                        "LAYER"            => { common.layer = value.clone(); changed += 1; }
                                        "LINETYPE" | "LT"  => { common.linetype = value.clone(); changed += 1; }
                                        "LTSCALE"          => { common.linetype_scale = ltscale_val.unwrap(); changed += 1; }
                                        "COLOR"            => { common.color = color_val.unwrap(); changed += 1; }
                                        "TRANSPARENCY"     => { common.transparency = transparency_val.unwrap(); changed += 1; }
                                        _ => {
                                            self.command_line.push_error(&format!(
                                                "CHPROP: unknown property '{}'. Use: LAYER COLOR LINETYPE LTSCALE TRANSPARENCY", prop
                                            ));
                                            break;
                                        }
                                    }
                                }
                            }
                            if changed > 0 {
                                self.push_undo_snapshot(i, "CHPROP");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!("CHPROP: {} entity/entities updated.", changed));
                            }
                        }
                    }
                }
            }

            // ── RENAME table entries ──────────────────────────────────────
            cmd if cmd == "RENAME" || cmd.starts_with("RENAME ") => {
                // Usage: RENAME <type> <old_name> <new_name>
                // Types: LAYER BLOCK STYLE DIMSTYLE LINETYPE UCS VIEW
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                let type_str = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
                let old_name = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
                let new_name = parts.get(3).map(|s| s.trim()).unwrap_or("").to_string();

                if type_str.is_empty() || old_name.is_empty() || new_name.is_empty() {
                    self.command_line.push_info(
                        "Usage: RENAME <type> <old> <new>  (types: LAYER BLOCK STYLE DIMSTYLE LINETYPE UCS VIEW)"
                    );
                } else {
                    let doc = &mut self.tabs[i].scene.document;
                    let ok = match type_str.as_str() {
                        "LAYER" => {
                            if let Some(l) = doc.layers.get_mut(&old_name) {
                                l.name = new_name.clone();
                                // Update entity references
                                for e in doc.entities_mut() {
                                    if e.common().layer == old_name {
                                        e.common_mut().layer = new_name.clone();
                                    }
                                }
                                true
                            } else { false }
                        }
                        "STYLE" | "TEXTSTYLE" => {
                            if let Some(s) = doc.text_styles.get_mut(&old_name) {
                                s.name = new_name.clone(); true
                            } else { false }
                        }
                        "DIMSTYLE" => {
                            if let Some(s) = doc.dim_styles.get_mut(&old_name) {
                                s.name = new_name.clone(); true
                            } else { false }
                        }
                        "LINETYPE" | "LT" => {
                            if let Some(lt) = doc.line_types.get_mut(&old_name) {
                                lt.name = new_name.clone(); true
                            } else { false }
                        }
                        "UCS" => {
                            if let Some(u) = doc.ucss.get_mut(&old_name) {
                                u.name = new_name.clone(); true
                            } else { false }
                        }
                        "VIEW" => {
                            if let Some(v) = doc.views.get_mut(&old_name) {
                                v.name = new_name.clone(); true
                            } else { false }
                        }
                        _ => {
                            self.command_line.push_error(&format!("RENAME: unknown type '{}'. Use LAYER BLOCK STYLE DIMSTYLE LINETYPE UCS VIEW", type_str));
                            false
                        }
                    };
                    if ok {
                        self.push_undo_snapshot(i, "RENAME");
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!("RENAME: '{}' → '{}'.", old_name, new_name));
                    } else if type_str != "BLOCK" {
                        self.command_line.push_error(&format!("RENAME: '{}' not found in {}.", old_name, type_str));
                    }
                }
            }

            // ── System variable getters/setters ──────────────────────────────────
            // CLAYER [name]    — get or set current layer
            // TEXTSTYLE [name] — already handled above under STYLE SET
            // DIMSTYLE [name]  — get or set active dim style
            // LTSCALE [val]    — global linetype scale
            cmd if cmd == "CLAYER" || cmd.starts_with("CLAYER ") => {
                let name_arg = cmd.trim_start_matches("CLAYER").trim();
                if name_arg.is_empty() {
                    let cur = &self.tabs[i].scene.document.header.current_layer_name;
                    self.command_line.push_output(&format!("CLAYER = \"{cur}\""));
                } else {
                    if self.tabs[i].scene.document.layers.contains(name_arg) {
                        self.tabs[i].scene.document.header.current_layer_name = name_arg.to_string();
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!("CLAYER set to \"{name_arg}\""));
                    } else {
                        self.command_line.push_error(&format!("CLAYER: layer '{}' not found.", name_arg));
                    }
                }
            }
            cmd if cmd == "CDIMSTY" || cmd == "DIMCURRENT" || cmd.starts_with("CDIMSTY ") || cmd.starts_with("DIMCURRENT ") => {
                let name_arg = cmd.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
                if name_arg.is_empty() {
                    let cur = &self.tabs[i].scene.document.header.current_dimstyle_name;
                    self.command_line.push_output(&format!("CDIMSTY = \"{cur}\""));
                } else {
                    if self.tabs[i].scene.document.dim_styles.contains(&name_arg) {
                        self.tabs[i].scene.document.header.current_dimstyle_name = name_arg.clone();
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!("Active dim style set to \"{name_arg}\""));
                    } else {
                        self.command_line.push_error(&format!("CDIMSTY: dim style '{}' not found.", name_arg));
                    }
                }
            }
            cmd if cmd == "LTSCALE" || cmd.starts_with("LTSCALE ") => {
                let val_str = cmd.trim_start_matches("LTSCALE").trim();
                if val_str.is_empty() {
                    let v = self.tabs[i].scene.document.header.linetype_scale;
                    self.command_line.push_output(&format!("LTSCALE = {v:.4}"));
                } else if let Ok(v) = val_str.parse::<f64>() {
                    if v > 0.0 {
                        self.push_undo_snapshot(i, "LTSCALE");
                        self.tabs[i].scene.document.header.linetype_scale = v;
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!("LTSCALE set to {v:.4}"));
                    } else {
                        self.command_line.push_error("LTSCALE: value must be positive.");
                    }
                } else {
                    self.command_line.push_error("Usage: LTSCALE [value]");
                }
            }
            cmd if cmd == "CELTSCALE" || cmd.starts_with("CELTSCALE ") => {
                let val_str = cmd.trim_start_matches("CELTSCALE").trim();
                if val_str.is_empty() {
                    let v = self.tabs[i].scene.document.header.current_entity_linetype_scale;
                    self.command_line.push_output(&format!("CELTSCALE = {v:.4}"));
                } else if let Ok(v) = val_str.parse::<f64>() {
                    if v > 0.0 {
                        self.tabs[i].scene.document.header.current_entity_linetype_scale = v;
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!("CELTSCALE set to {v:.4}"));
                    } else {
                        self.command_line.push_error("CELTSCALE: value must be positive.");
                    }
                } else {
                    self.command_line.push_error("Usage: CELTSCALE [value]");
                }
            }

            // ── SCALETEXT — rescale selected Text/MText entities ─────────────────
            // Usage: SCALETEXT <factor>   e.g. SCALETEXT 2
            //        SCALETEXT H <height>  set absolute height
            cmd if cmd == "SCALETEXT" || cmd.starts_with("SCALETEXT ") => {
                let rest = cmd.trim_start_matches("SCALETEXT").trim();
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let selected_handles: Vec<acadrust::Handle> = self.tabs[i].scene
                    .selected_entities()
                    .iter()
                    .map(|(h, _)| *h)
                    .collect();
                if selected_handles.is_empty() {
                    self.command_line.push_error("SCALETEXT: select Text/MText entities first.");
                } else {
                    let (use_absolute, value) = match (parts.first().map(|s| s.to_uppercase()).as_deref(), parts.get(1)) {
                        (Some("H"), Some(v)) => (true, v.parse::<f64>().ok()),
                        (Some(v), None) => (false, v.parse::<f64>().ok()),
                        _ => (false, None),
                    };
                    if let Some(val) = value {
                        if val <= 0.0 {
                            self.command_line.push_error("SCALETEXT: value must be positive.");
                        } else {
                            self.push_undo_snapshot(i, "SCALETEXT");
                            let mut count = 0usize;
                            for sh in &selected_handles {
                                for entity in self.tabs[i].scene.document.entities_mut() {
                                    if entity.common().handle != *sh { continue; }
                                    match entity {
                                        acadrust::EntityType::Text(t) => {
                                            t.height = if use_absolute { val } else { t.height * val };
                                            count += 1;
                                        }
                                        acadrust::EntityType::MText(t) => {
                                            t.height = if use_absolute { val } else { t.height * val };
                                            count += 1;
                                        }
                                        _ => {}
                                    }
                                    break;
                                }
                            }
                            if count > 0 {
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!(
                                    "SCALETEXT: scaled {count} text entity(ies)."
                                ));
                            } else {
                                self.command_line.push_error("SCALETEXT: no Text/MText in selection.");
                            }
                        }
                    } else {
                        self.command_line.push_info("Usage: SCALETEXT <factor>  or  SCALETEXT H <height>");
                    }
                }
            }

            // ── Display refresh (no-op in GPU raster pipeline) ────────────────
            "REGEN"|"REGENALL"|"REDRAW"|"REDRWALL" => {
                // Display is always up-to-date in the GPU raster pipeline.
                self.command_line.push_output("Display regenerated.");
            }

            // ── TABLE cell editing ─────────────────────────────────────────────
            // TABLE CELL <row> <col> <text> — set text for a cell in the selected Table
            cmd if cmd.starts_with("TABLE ") => {
                let rest = cmd.trim_start_matches("TABLE").trim();
                let sub_up = rest.split_whitespace().next().unwrap_or("").to_uppercase();
                if sub_up == "CELL" {
                    let parts: Vec<&str> = rest.splitn(4, char::is_whitespace).collect();
                    // parts: ["CELL", "<row>", "<col>", "<text>"]
                    let row_res = parts.get(1).and_then(|s| s.parse::<usize>().ok());
                    let col_res = parts.get(2).and_then(|s| s.parse::<usize>().ok());
                    let text = parts.get(3).copied().unwrap_or("");
                    match (row_res, col_res) {
                        (Some(row), Some(col)) => {
                            let selected_handles: Vec<acadrust::Handle> = self.tabs[i].scene
                                .selected_entities()
                                .iter()
                                .map(|(h, _)| *h)
                                .collect();
                            let mut found = false;
                            for sh in &selected_handles {
                                if let Some(acadrust::EntityType::Table(tbl)) =
                                    self.tabs[i].scene.document.entities_mut().find(|e| e.common().handle == *sh)
                                {
                                    if tbl.set_cell_text(row, col, text) {
                                        found = true;
                                    }
                                }
                            }
                            if found {
                                self.push_undo_snapshot(i, "TABLE CELL");
                                self.tabs[i].dirty = true;
                                self.command_line.push_output(&format!(
                                    "TABLE CELL: set [{row},{col}] = \"{text}\"."
                                ));
                            } else {
                                self.command_line.push_error(
                                    "TABLE CELL: select a Table entity first, or row/col out of range."
                                );
                            }
                        }
                        _ => {
                            self.command_line.push_info("Usage: TABLE CELL <row> <col> <text>");
                        }
                    }
                } else {
                    self.command_line.push_info("Usage: TABLE  (creates new table)  or  TABLE CELL <row> <col> <text>");
                }
            }

            // ── UCSICON — toggle UCS icon visibility on all viewports ────────────
            // UCSICON ON       — show UCS icon in all viewports
            // UCSICON OFF      — hide UCS icon in all viewports
            // UCSICON NOORIGIN — show icon but not at origin (show at corner)
            // UCSICON ORIGIN   — show icon at UCS origin
            cmd if cmd == "UCSICON" || cmd.starts_with("UCSICON ") => {
                let sub = cmd.split_whitespace().nth(1).unwrap_or("").to_uppercase();
                match sub.as_str() {
                    "ON" | "OFF" | "NOORIGIN" | "ORIGIN" => {
                        self.push_undo_snapshot(i, "UCSICON");
                        let visible = sub != "OFF";
                        let at_origin = sub == "ORIGIN";
                        // Update model-space icon flag.
                        self.show_ucs_icon = visible;
                        let mut count = 0usize;
                        for entity in self.tabs[i].scene.document.entities_mut() {
                            if let acadrust::EntityType::Viewport(vp) = entity {
                                vp.status.ucs_icon_visible = visible;
                                if sub == "NOORIGIN" || sub == "ORIGIN" {
                                    vp.status.ucs_icon_at_origin = at_origin;
                                }
                                count += 1;
                            }
                        }
                        self.tabs[i].dirty = true;
                        self.command_line.push_output(&format!(
                            "UCSICON {sub}: updated {count} viewport(s) + model space."
                        ));
                    }
                    "" => {
                        // Bare UCSICON toggles visibility.
                        self.push_undo_snapshot(i, "UCSICON");
                        let visible = !self.show_ucs_icon;
                        self.show_ucs_icon = visible;
                        for entity in self.tabs[i].scene.document.entities_mut() {
                            if let acadrust::EntityType::Viewport(vp) = entity {
                                vp.status.ucs_icon_visible = visible;
                            }
                        }
                        self.tabs[i].dirty = true;
                        let state = if visible { "ON" } else { "OFF" };
                        self.command_line.push_output(&format!("UCSICON {state}"));
                    }
                    _ => {
                        self.command_line.push_info("Usage: UCSICON ON | OFF | NOORIGIN | ORIGIN");
                    }
                }
            }

            // ── NAVVCUBE — toggle ViewCube visibility ────────────────────────────
            "NAVVCUBE" => {
                return Task::done(Message::ToggleViewCube);
            }

            // ── NAVBAR — toggle navigation toolbar visibility ────────────────────
            "NAVBAR" => {
                return Task::done(Message::ToggleNavbar);
            }

            // ── PROPERTIES — toggle Properties panel visibility ──────────────────
            "PROPERTIES" | "PR" | "PROPS" => {
                return Task::done(Message::ToggleProperties);
            }

            // ── FILETAB — toggle file/document tabs ──────────────────────────────
            "FILETAB" => {
                return Task::done(Message::ToggleFileTabs);
            }

            // ── LAYOUTTAB — toggle layout/paper-space tabs ───────────────────────
            "LAYOUTTAB" => {
                return Task::done(Message::ToggleLayoutTabs);
            }

            // ── TOOLPALETTES — not yet implemented ───────────────────────────────
            "TOOLPALETTES" | "TP" => {
                self.command_line.push_info("TOOLPALETTES: Tool Palettes not yet implemented.");
            }

            // ── SHEETSET — not yet implemented ───────────────────────────────────
            "SHEETSET" | "SSM" => {
                self.command_line.push_info("SHEETSET: Sheet Set Manager not yet implemented.");
            }

            // ── XDATA — read/write extended entity data ──────────────────────────
            // XDATA LIST             — show all xdata records on selected entities
            // XDATA SET <app> <str>  — append a string xdata value for <app>
            // XDATA CLEAR            — remove all xdata from selected entities
            // XDATA CLEAR <app>      — remove xdata for a specific application
            cmd if cmd == "XDATA" || cmd.starts_with("XDATA ") => {
                use acadrust::xdata::{ExtendedDataRecord, XDataValue};
                let rest = cmd.trim_start_matches("XDATA").trim();
                let parts: Vec<&str> = rest.splitn(3, char::is_whitespace).collect();
                let sub = parts.first().map(|s| s.to_uppercase()).unwrap_or_default();
                let selected_handles: Vec<acadrust::Handle> = self.tabs[i].scene
                    .selected_entities()
                    .iter()
                    .map(|(h, _)| *h)
                    .collect();
                if selected_handles.is_empty() {
                    self.command_line.push_error("XDATA: select entities first.");
                } else {
                    match sub.as_str() {
                        "LIST" | "" => {
                            for sh in &selected_handles {
                                if let Some(entity) = self.tabs[i].scene.document.get_entity(*sh) {
                                    let xd = &entity.common().extended_data;
                                    if xd.is_empty() {
                                        self.command_line.push_output(&format!("  {:x}: no xdata.", sh.value()));
                                    } else {
                                        for rec in xd.records() {
                                            self.command_line.push_output(&format!(
                                                "  {:x} [{}]: {} value(s)", sh.value(), rec.application_name, rec.values.len()
                                            ));
                                            for v in &rec.values {
                                                self.command_line.push_output(&format!("    {:?}", v));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "SET" => {
                            let app = parts.get(1).copied().unwrap_or("H7CAD");
                            let val = parts.get(2).copied().unwrap_or("");
                            self.push_undo_snapshot(i, "XDATA SET");
                            for sh in &selected_handles {
                                if let Some(entity) = self.tabs[i].scene.document.get_entity_mut(*sh) {
                                    let mut rec = ExtendedDataRecord::new(app);
                                    rec.add_value(XDataValue::String(val.to_string()));
                                    entity.common_mut().extended_data.add_record(rec);
                                }
                            }
                            self.tabs[i].dirty = true;
                            self.command_line.push_output(&format!(
                                "XDATA: set [{app}] = \"{val}\" on {} entity/entities.", selected_handles.len()
                            ));
                        }
                        "CLEAR" => {
                            let app_filter = parts.get(1).copied();
                            self.push_undo_snapshot(i, "XDATA CLEAR");
                            for sh in &selected_handles {
                                if let Some(entity) = self.tabs[i].scene.document.get_entity_mut(*sh) {
                                    let xd = &mut entity.common_mut().extended_data;
                                    if let Some(app) = app_filter {
                                        // Rebuild without the matching app.
                                        let kept: Vec<_> = xd.records().iter()
                                            .filter(|r| r.application_name != app)
                                            .cloned()
                                            .collect();
                                        xd.clear();
                                        for r in kept { xd.add_record(r); }
                                    } else {
                                        xd.clear();
                                    }
                                }
                            }
                            self.tabs[i].dirty = true;
                            self.command_line.push_output("XDATA: cleared.");
                        }
                        _ => {
                            self.command_line.push_info("Usage: XDATA LIST | SET <app> <value> | CLEAR [app]");
                        }
                    }
                }
            }

            // ── 3D Primitive — BOX ────────────────────────────────────────
            "BOX" => {
                use crate::modules::insert::solid3d_cmds::BoxCommand;
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                let cmd = BoxCommand::new(color);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── 3D Primitive — SPHERE ─────────────────────────────────────
            "SPHERE" => {
                use crate::modules::insert::solid3d_cmds::SphereCommand;
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                let cmd = SphereCommand::new(color);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── 3D Primitive — CYLINDER ───────────────────────────────────
            "CYLINDER" => {
                use crate::modules::insert::solid3d_cmds::CylinderCommand;
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                let cmd = CylinderCommand::new(color);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── EXTRUDE ────────────────────────────────────────────────────
            "EXTRUDE"|"EXT" => {
                use crate::modules::insert::solid3d_cmds::ExtrudeCommand;
                // If a single entity is already selected, skip the pick step.
                let selected: Vec<_> = self.tabs[i].scene.selected_entities().into_iter().collect();
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                if selected.len() == 1 {
                    let handle = selected[0].0;
                    let mut cmd = ExtrudeCommand::new(color);
                    cmd.on_entity_pick(handle, glam::Vec3::ZERO);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let cmd = ExtrudeCommand::new(color);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            // ── REVOLVE ────────────────────────────────────────────────────
            "REVOLVE"|"REV" => {
                use crate::modules::insert::solid3d_cmds::RevolveCommand;
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                let cmd = RevolveCommand::new(color);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── SWEEP ──────────────────────────────────────────────────────
            "SWEEP" => {
                use crate::modules::insert::solid3d_cmds::SweepCommand;
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                let cmd = SweepCommand::new(color);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── LOFT ───────────────────────────────────────────────────────
            "LOFT" => {
                use crate::modules::insert::solid3d_cmds::LoftCommand;
                let color = self.tabs[i].scene.layer_color(&self.tabs[i].active_layer);
                let cmd = LoftCommand::new(color);
                self.command_line.push_info(&cmd.prompt());
                self.tabs[i].active_cmd = Some(Box::new(cmd));
            }

            // ── OBJ import ───────────────────────────────────────────────
            "IMPORTOBJ"|"OBJIMPORT" => {
                return Task::done(Message::ObjImport);
            }

            // ── STL export ────────────────────────────────────────────────
            "STLOUT"|"EXPORTSTL" => {
                return Task::done(Message::StlExport);
            }

            // STEPOUT — export 3D meshes to STEP AP203 format
            "STEPOUT"|"EXPORTSTEP"|"STPOUT" => {
                return Task::done(Message::StepExport);
            }

            // ── Plot Style Editor GUI ─────────────────────────────────────
            "PLOTSTYLEPANEL"|"PLOTSTYLEEDITOR"|"STYLESMANAGER" => {
                return Task::done(Message::PlotStylePanelOpen);
            }

            // ── Plot / Page Setup ──────────────────────────────────────────
            "PLOT"|"EXPORT" => {
                return Task::done(Message::PlotExport);
            }
            // PRINT — send current layout to the system default printer.
            "PRINT" => {
                return Task::done(Message::PrintToPrinter);
            }
            // PLOTSTYLE — load or clear CTB/STB plot style table
            cmd if cmd == "PLOTSTYLE" || cmd.starts_with("PLOTSTYLE ") => {
                let sub = cmd.split_once(' ')
                    .map(|(_, r)| r.trim().to_uppercase())
                    .unwrap_or_default();
                match sub.as_str() {
                    "CLEAR" | "NONE" => {
                        return Task::done(Message::PlotStyleClear);
                    }
                    "" | "LOAD" => {
                        let active = self.active_plot_style.as_ref()
                            .map(|t| format!("Active: {}", t.name))
                            .unwrap_or_else(|| "No plot style loaded.".into());
                        self.command_line.push_info(&active);
                        return Task::done(Message::PlotStyleLoad);
                    }
                    "?" | "STATUS" => {
                        let msg = self.active_plot_style.as_ref()
                            .map(|t| format!(
                                "Plot style: {}  ({} color overrides)",
                                t.name,
                                t.aci_entries.iter().filter(|e| e.color.is_some()).count()
                            ))
                            .unwrap_or_else(|| "No plot style table loaded.".into());
                        self.command_line.push_output(&msg);
                    }
                    _ => {
                        self.command_line.push_error(
                            "Usage: PLOTSTYLE [LOAD | CLEAR | STATUS]"
                        );
                    }
                }
            }
            // UNDERLAY — edit properties of selected PDF/DWF/DGN underlay entities.
            // Usage:
            //   UNDERLAY FADE <0-80>
            //   UNDERLAY CONTRAST <0-100>
            //   UNDERLAY ON | OFF
            //   UNDERLAY CLIP ON | OFF
            //   UNDERLAY MONO ON | OFF
            cmd if cmd == "UNDERLAY" || cmd.starts_with("UNDERLAY ") => {
                let sub = cmd.split_once(' ')
                    .map(|(_, r)| r.trim().to_uppercase())
                    .unwrap_or_default();
                let handles: Vec<acadrust::Handle> = self.tabs[i].scene
                    .selected_entities()
                    .iter()
                    .map(|(h, _)| *h)
                    .collect();
                if handles.is_empty() {
                    self.command_line.push_error("UNDERLAY: select underlay entities first.");
                } else {
                    let parts: Vec<&str> = sub.splitn(2, char::is_whitespace).collect();
                    let action = parts.first().copied().unwrap_or("");
                    let arg = parts.get(1).copied().unwrap_or("").trim();
                    let mut changed = 0usize;
                    self.push_undo_snapshot(i, "UNDERLAY");
                    for h in &handles {
                        if let Some(acadrust::EntityType::Underlay(ul)) = self.tabs[i].scene
                            .document.entities_mut()
                            .find(|e| e.common().handle == *h)
                        {
                            match action {
                                "FADE" => {
                                    if let Ok(v) = arg.parse::<u8>() {
                                        ul.set_fade(v);
                                        changed += 1;
                                    }
                                }
                                "CONTRAST" => {
                                    if let Ok(v) = arg.parse::<u8>() {
                                        ul.set_contrast(v);
                                        changed += 1;
                                    }
                                }
                                "ON" => { ul.set_on(true); changed += 1; }
                                "OFF" => { ul.set_on(false); changed += 1; }
                                "CLIP" => {
                                    match arg {
                                        "ON" => {
                                            ul.flags |= acadrust::entities::UnderlayDisplayFlags::CLIPPING;
                                            changed += 1;
                                        }
                                        "OFF" => {
                                            ul.clear_clip();
                                            changed += 1;
                                        }
                                        _ => {}
                                    }
                                }
                                "MONO" => {
                                    match arg {
                                        "ON" => { ul.set_monochrome(true); changed += 1; }
                                        "OFF" => { ul.set_monochrome(false); changed += 1; }
                                        _ => {}
                                    }
                                }
                                _ => {
                                    // No sub-command: print status.
                                    self.command_line.push_output(&format!(
                                        "Underlay {:x}: fade={}, contrast={}, on={}, clip={}, mono={}",
                                        h.value(),
                                        ul.fade,
                                        ul.contrast,
                                        ul.is_on(),
                                        ul.is_clipping(),
                                        ul.is_monochrome(),
                                    ));
                                }
                            }
                        }
                    }
                    if changed > 0 {
                        self.tabs[i].dirty = true;
                        self.command_line.push_info(&format!(
                            "Updated {changed} underlay(s)."
                        ));
                    } else if !action.is_empty() {
                        self.command_line.push_error(
                            "Usage: UNDERLAY [FADE <n>|CONTRAST <n>|ON|OFF|CLIP ON|OFF|MONO ON|OFF]"
                        );
                    }
                }
            }

            "PAGESETUP" => {
                if self.tabs[i].scene.current_layout == "Model" {
                    self.command_line.push_error("PAGESETUP: switch to a paper space layout first.");
                } else {
                    return Task::done(Message::PageSetupOpen);
                }
            }

            _ => self.command_line.push_error(&format!("Unknown command: {cmd}")),
        }

        // Focus the command line whenever a command just became active.
        let i = self.active_tab;
        if self.tabs[i].active_cmd.is_some() {
            self.tabs[i].last_cmd = Some(cmd.to_string());
            self.focus_cmd_input()
        } else {
            Task::none()
        }
    }
}

// ── FIND/REPLACE helpers ───────────────────────────────────────────────────

fn entity_list_details(entity: &acadrust::EntityType) -> String {
    use std::f64::consts::PI;
    match entity {
        acadrust::EntityType::Line(l) => format!(
            "from ({:.4},{:.4},{:.4}) to ({:.4},{:.4},{:.4})  len={:.4}",
            l.start.x, l.start.y, l.start.z,
            l.end.x, l.end.y, l.end.z,
            ((l.end.x-l.start.x).powi(2)+(l.end.y-l.start.y).powi(2)+(l.end.z-l.start.z).powi(2)).sqrt()
        ),
        acadrust::EntityType::Circle(c) => format!(
            "center ({:.4},{:.4},{:.4})  r={:.4}  area={:.4}",
            c.center.x, c.center.y, c.center.z, c.radius,
            PI * c.radius * c.radius
        ),
        acadrust::EntityType::Arc(a) => format!(
            "center ({:.4},{:.4},{:.4})  r={:.4}  start={:.2}° end={:.2}°",
            a.center.x, a.center.y, a.center.z, a.radius, a.start_angle, a.end_angle
        ),
        acadrust::EntityType::LwPolyline(p) => format!(
            "{} vertices  closed={}  elevation={:.4}",
            p.vertices.len(), p.is_closed, p.elevation
        ),
        acadrust::EntityType::Text(t) => format!(
            "\"{}\"  h={:.4}  at ({:.4},{:.4})",
            t.value, t.height, t.insertion_point.x, t.insertion_point.y
        ),
        acadrust::EntityType::MText(t) => format!(
            "\"{}\"  h={:.4}  at ({:.4},{:.4})",
            t.value.chars().take(40).collect::<String>(),
            t.height, t.insertion_point.x, t.insertion_point.y
        ),
        acadrust::EntityType::Insert(ins) => format!(
            "block=\"{}\"  at ({:.4},{:.4},{:.4})  scale=({:.4},{:.4},{:.4})  rot={:.2}°",
            ins.block_name, ins.insert_point.x, ins.insert_point.y, ins.insert_point.z,
            ins.x_scale(), ins.y_scale(), ins.z_scale(),
            ins.rotation.to_degrees()
        ),
        acadrust::EntityType::Spline(s) => format!(
            "{} ctrl pts  degree={}  closed={}",
            s.control_points.len(), s.degree, s.flags.closed
        ),
        acadrust::EntityType::Ellipse(e) => format!(
            "center ({:.4},{:.4})  major_len={:.4}  ratio={:.4}",
            e.center.x, e.center.y, e.major_axis_length(), e.minor_axis_ratio
        ),
        _ => String::new(),
    }
}

fn flatten_entity_z(entity: &mut acadrust::EntityType) {
    match entity {
        acadrust::EntityType::Line(l)        => { l.start.z = 0.0; l.end.z = 0.0; }
        acadrust::EntityType::Circle(c)      => { c.center.z = 0.0; }
        acadrust::EntityType::Arc(a)         => { a.center.z = 0.0; }
        acadrust::EntityType::LwPolyline(p)  => { p.elevation = 0.0; }
        acadrust::EntityType::Text(t)        => { t.insertion_point.z = 0.0; }
        acadrust::EntityType::MText(t)       => { t.insertion_point.z = 0.0; }
        acadrust::EntityType::Insert(ins)    => { ins.insert_point.z = 0.0; }
        acadrust::EntityType::Point(p)       => { p.location.z = 0.0; }
        acadrust::EntityType::Spline(s)      => {
            for cp in &mut s.control_points { cp.z = 0.0; }
            for fp in &mut s.fit_points     { fp.z = 0.0; }
        }
        acadrust::EntityType::Ellipse(e)     => { e.center.z = 0.0; }
        _ => {}
    }
}

/// Find the last placed linear or aligned dimension in the document.
/// Returns `(first_point, second_point, definition_point, rotation_rad)` in world-space.
fn find_last_linear_dim(scene: &crate::scene::Scene) -> Option<(glam::Vec3, glam::Vec3, glam::Vec3, f64)> {
    use acadrust::entities::Dimension;
    let mut best_handle: u64 = 0;
    let mut result: Option<(glam::Vec3, glam::Vec3, glam::Vec3, f64)> = None;

    for entity in scene.document.entities() {
        if let acadrust::EntityType::Dimension(dim) = entity {
            let h = entity.common().handle.value();
            if h <= best_handle {
                continue;
            }
            let item = match dim {
                Dimension::Linear(d) => {
                    let p1 = glam::Vec3::new(d.first_point.x as f32, d.first_point.y as f32, d.first_point.z as f32);
                    let p2 = glam::Vec3::new(d.second_point.x as f32, d.second_point.y as f32, d.second_point.z as f32);
                    let dp = glam::Vec3::new(d.base.definition_point.x as f32, d.base.definition_point.y as f32, d.base.definition_point.z as f32);
                    Some((p1, p2, dp, d.rotation))
                }
                Dimension::Aligned(d) => {
                    let p1 = glam::Vec3::new(d.first_point.x as f32, d.first_point.y as f32, d.first_point.z as f32);
                    let p2 = glam::Vec3::new(d.second_point.x as f32, d.second_point.y as f32, d.second_point.z as f32);
                    let dp = glam::Vec3::new(d.base.definition_point.x as f32, d.base.definition_point.y as f32, d.base.definition_point.z as f32);
                    let dx = (d.second_point.x - d.first_point.x) as f32;
                    let dy = (d.second_point.y - d.first_point.y) as f32;
                    let rot = dy.atan2(dx) as f64;
                    Some((p1, p2, dp, rot))
                }
                _ => None,
            };
            if let Some(data) = item {
                best_handle = h;
                result = Some(data);
            }
        }
    }
    result
}

fn entity_type_name(entity: &acadrust::EntityType) -> &'static str {
    match entity {
        acadrust::EntityType::Line(_)               => "LINE",
        acadrust::EntityType::Circle(_)             => "CIRCLE",
        acadrust::EntityType::Arc(_)                => "ARC",
        acadrust::EntityType::LwPolyline(_)         => "LWPOLYLINE",
        acadrust::EntityType::Polyline(_)           => "POLYLINE",
        acadrust::EntityType::Polyline2D(_)         => "POLYLINE2D",
        acadrust::EntityType::Polyline3D(_)         => "POLYLINE3D",
        acadrust::EntityType::Text(_)               => "TEXT",
        acadrust::EntityType::MText(_)              => "MTEXT",
        acadrust::EntityType::Insert(_)             => "INSERT",
        acadrust::EntityType::Hatch(_)              => "HATCH",
        acadrust::EntityType::Dimension(_)          => "DIMENSION",
        acadrust::EntityType::Viewport(_)           => "VIEWPORT",
        acadrust::EntityType::Spline(_)             => "SPLINE",
        acadrust::EntityType::Ellipse(_)            => "ELLIPSE",
        acadrust::EntityType::Point(_)              => "POINT",
        acadrust::EntityType::Ray(_)                => "RAY",
        acadrust::EntityType::XLine(_)              => "XLINE",
        acadrust::EntityType::Face3D(_)             => "3DFACE",
        acadrust::EntityType::Table(_)              => "TABLE",
        acadrust::EntityType::MLine(_)              => "MLINE",
        acadrust::EntityType::RasterImage(_)        => "RASTERIMAGE",
        acadrust::EntityType::Wipeout(_)            => "WIPEOUT",
        acadrust::EntityType::Underlay(_)           => "UNDERLAY",
        acadrust::EntityType::AttributeDefinition(_)=> "ATTDEF",
        acadrust::EntityType::AttributeEntity(_)    => "ATTRIB",
        acadrust::EntityType::Leader(_)             => "LEADER",
        acadrust::EntityType::Tolerance(_)          => "TOLERANCE",
        acadrust::EntityType::Shape(_)              => "SHAPE",
        _ => "ENTITY",
    }
}

fn entity_text_content(entity: &acadrust::EntityType) -> Option<String> {
    match entity {
        acadrust::EntityType::Text(t)  => Some(t.value.clone()),
        acadrust::EntityType::MText(t) => Some(t.value.clone()),
        acadrust::EntityType::AttributeDefinition(a) => Some(a.default_value.clone()),
        acadrust::EntityType::AttributeEntity(a) => Some(a.get_value().to_string()),
        _ => None,
    }
}

// ── MASSPROP helpers ───────────────────────────────────────────────────────

struct MassProps {
    area: f64,
    perimeter: f64,
    cx: f64,
    cy: f64,
}

fn massprop_entity(entity: &acadrust::EntityType) -> Option<MassProps> {
    use std::f64::consts::{PI, TAU};

    match entity {
        acadrust::EntityType::Circle(c) => {
            let r = c.radius;
            Some(MassProps {
                area: PI * r * r,
                perimeter: TAU * r,
                cx: c.center.x,
                cy: c.center.y,
            })
        }
        acadrust::EntityType::Arc(a) => {
            let r = a.radius;
            let span = {
                let s = ((a.end_angle - a.start_angle) + 360.0) % 360.0;
                if s < 1e-6 { 360.0 } else { s }
            };
            let span_rad = span.to_radians();
            // Sector area (pie slice)
            let area = 0.5 * r * r * span_rad;
            let arc_len = r * span_rad;
            // Centroid of arc (chord midpoint direction)
            let mid_rad = (a.start_angle + span / 2.0).to_radians();
            Some(MassProps {
                area,
                perimeter: arc_len,
                cx: a.center.x + r * mid_rad.cos(),
                cy: a.center.y + r * mid_rad.sin(),
            })
        }
        acadrust::EntityType::Line(l) => {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            let len = (dx * dx + dy * dy).sqrt();
            Some(MassProps {
                area: 0.0,
                perimeter: len,
                cx: (l.start.x + l.end.x) / 2.0,
                cy: (l.start.y + l.end.y) / 2.0,
            })
        }
        acadrust::EntityType::LwPolyline(p) => {
            let n = p.vertices.len();
            if n < 2 { return None; }
            // Shoelace area + perimeter
            let mut area_sum = 0.0f64;
            let mut perimeter = 0.0f64;
            let mut cx_sum = 0.0f64;
            let mut cy_sum = 0.0f64;
            let n_segs = if p.is_closed { n } else { n - 1 };
            for idx in 0..n_segs {
                let v0 = &p.vertices[idx];
                let v1 = &p.vertices[(idx + 1) % n];
                let x0 = v0.location.x;
                let y0 = v0.location.y;
                let x1 = v1.location.x;
                let y1 = v1.location.y;
                area_sum += x0 * y1 - x1 * y0;
                perimeter += ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                cx_sum += (x0 + x1) * (x0 * y1 - x1 * y0);
                cy_sum += (y0 + y1) * (x0 * y1 - x1 * y0);
            }
            let area = (area_sum / 2.0).abs();
            let (cx, cy) = if area > 1e-12 {
                (cx_sum / (6.0 * area), cy_sum / (6.0 * area))
            } else {
                let sx: f64 = p.vertices.iter().map(|v| v.location.x).sum::<f64>() / n as f64;
                let sy: f64 = p.vertices.iter().map(|v| v.location.y).sum::<f64>() / n as f64;
                (sx, sy)
            };
            Some(MassProps { area, perimeter, cx, cy })
        }
        acadrust::EntityType::Ellipse(e) => {
            let a = (e.major_axis.x.powi(2) + e.major_axis.y.powi(2)).sqrt();
            let b = a * e.minor_axis_ratio;
            let t0 = e.start_parameter;
            let t1 = {
                let mut t = e.end_parameter;
                if t <= t0 { t += TAU; }
                t
            };
            let span = t1 - t0;
            let is_full = (span - TAU).abs() < 1e-6;
            let area = if is_full {
                PI * a * b
            } else {
                // Sector area of ellipse approximated via 256-pt integration
                let n = 256usize;
                let mut s = 0.0f64;
                for k in 0..n {
                    let t = t0 + span * (k as f64 / n as f64);
                    let tp = t0 + span * ((k + 1) as f64 / n as f64);
                    let nx = e.major_axis.x / a;
                    let ny = e.major_axis.y / a;
                    let x0 = a * t.cos() * nx - b * t.sin() * ny;
                    let y0 = a * t.cos() * ny + b * t.sin() * nx;
                    let x1 = a * tp.cos() * nx - b * tp.sin() * ny;
                    let y1 = a * tp.cos() * ny + b * tp.sin() * nx;
                    s += x0 * y1 - x1 * y0;
                }
                (s / 2.0).abs()
            };
            // Arc length via 256-pt numerical integration
            let nx = e.major_axis.x / a.max(1e-12);
            let ny = e.major_axis.y / a.max(1e-12);
            let perimeter = {
                let n = 256usize;
                let mut len = 0.0f64;
                for k in 0..n {
                    let t = t0 + span * (k as f64 / n as f64);
                    let tp = t0 + span * ((k + 1) as f64 / n as f64);
                    let x0 = e.center.x + a * t.cos() * nx - b * t.sin() * ny;
                    let y0 = e.center.y + a * t.cos() * ny + b * t.sin() * nx;
                    let x1 = e.center.x + a * tp.cos() * nx - b * tp.sin() * ny;
                    let y1 = e.center.y + a * tp.cos() * ny + b * tp.sin() * nx;
                    len += (x1 - x0).hypot(y1 - y0);
                }
                len
            };
            Some(MassProps { area, perimeter, cx: e.center.x, cy: e.center.y })
        }
        _ => None,
    }
}

fn replace_entity_text(entity: &mut acadrust::EntityType, search: &str, rep: &str) {
    let search_lc = search.to_lowercase();
    match entity {
        acadrust::EntityType::Text(t) => {
            if t.value.to_lowercase().contains(&search_lc) {
                t.value = t.value.replace(search, rep);
            }
        }
        acadrust::EntityType::MText(t) => {
            if t.value.to_lowercase().contains(&search_lc) {
                t.value = t.value.replace(search, rep);
            }
        }
        acadrust::EntityType::AttributeDefinition(a) => {
            if a.default_value.to_lowercase().contains(&search_lc) {
                a.default_value = a.default_value.replace(search, rep);
            }
        }
        acadrust::EntityType::AttributeEntity(a) => {
            let cur = a.get_value().to_string();
            if cur.to_lowercase().contains(&search_lc) {
                a.set_value(cur.replace(search, rep));
            }
        }
        _ => {}
    }
}


// ── DATAEXTRACTION ─────────────────────────────────────────────────────────

/// Build a CSV string with one row per entity in model space.
/// Columns: Type, Handle, Layer, Color, Linetype, ExtraInfo
fn build_data_extraction_csv(doc: &acadrust::CadDocument) -> String {
    use acadrust::EntityType;

    let mut out = String::from("Type,Handle,Layer,Color,Linetype,ExtraInfo\n");

    let ms_handle = doc.header.model_space_block_handle;
    for e in doc.entities() {
        // Skip Block/EndBlock sentinels and paper-space entities.
        if matches!(e, EntityType::Block(_) | EntityType::BlockEnd(_)) {
            continue;
        }
        if !ms_handle.is_null() && e.common().owner_handle != ms_handle {
            continue;
        }
        let type_name = entity_type_name(e);
        let handle = format!("{:X}", e.common().handle.value());
        let layer = csv_escape(&e.common().layer);
        let color = format!("{}", e.common().color);
        let lt = csv_escape(&e.common().linetype);
        let extra = csv_escape(&entity_extra_info(e));
        out.push_str(&format!("{type_name},{handle},{layer},{color},{lt},{extra}\n"));
    }
    out
}

/// Return a short geometry summary for CSV ExtraInfo column.
fn entity_extra_info(entity: &acadrust::EntityType) -> String {
    use acadrust::EntityType;
    match entity {
        EntityType::Line(e) => format!(
            "({:.3},{:.3})-({:.3},{:.3})",
            e.start.x, e.start.y, e.end.x, e.end.y
        ),
        EntityType::Circle(e) => format!(
            "C({:.3},{:.3}) R={:.3}",
            e.center.x, e.center.y, e.radius
        ),
        EntityType::Arc(e) => format!(
            "C({:.3},{:.3}) R={:.3} {:.1}°-{:.1}°",
            e.center.x, e.center.y, e.radius, e.start_angle, e.end_angle
        ),
        EntityType::Text(e) => e.value.clone(),
        EntityType::MText(e) => e.value.chars().take(60).collect(),
        EntityType::Insert(e) => format!("BLK={} @({:.3},{:.3})", e.block_name, e.insert_point.x, e.insert_point.y),
        EntityType::LwPolyline(e) => format!("{} vertices", e.vertices.len()),
        EntityType::Polyline(e) => format!("{} vertices", e.vertices.len()),
        EntityType::Polyline2D(e) => format!("{} vertices", e.vertices.len()),
        EntityType::Polyline3D(e) => format!("{} vertices", e.vertices.len()),
        EntityType::Hatch(e) => format!("PAT={}", e.pattern.name),
        EntityType::Dimension(e) => format!("{:.3}", e.base().actual_measurement),
        EntityType::Spline(e) => format!("{} ctrl pts", e.control_points.len()),
        _ => String::new(),
    }
}

/// Escape a string for a CSV field (wrap in quotes if it contains comma/quote/newline).
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
