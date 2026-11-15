//! Centered popup dialogs: add folder, edit rule, confirm remove, preview.

use std::path::PathBuf;

use autosort_core::template::{TOKEN_HELP, expand_tilde};
use autosort_core::*;
use eframe::egui::{self, Align, Color32, Frame, Id, Layout, Margin, Modal, RichText, Stroke, Ui, vec2};
use uuid::Uuid;

use crate::app::{AddFolderForm, AutoSortApp, ModalState};
use crate::theme as t;
use crate::widgets::*;

fn frame() -> Frame {
    Frame::new()
        .fill(t::SURFACE)
        .stroke(Stroke::new(1.0, t::BORDER_HI))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(Margin::same(26))
        .shadow(egui::Shadow { offset: [0, 14], blur: 44, spread: 0, color: Color32::from_black_alpha(160) })
}

fn modal(id: impl std::hash::Hash + std::fmt::Debug) -> Modal {
    Modal::new(Id::unique(id)).frame(frame()).backdrop_color(Color32::from_black_alpha(170))
}

fn title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(20.0).strong().color(t::TEXT));
    if !subtitle.is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(subtitle).color(t::DIM));
    }
    ui.add_space(14.0);
}

fn quick_dirs() -> Vec<(&'static str, PathBuf)> {
    let Some(u) = directories::UserDirs::new() else { return vec![] };
    let mut v = Vec::new();
    if let Some(p) = u.download_dir() {
        v.push(("Downloads", p.to_path_buf()));
    }
    if let Some(p) = u.desktop_dir() {
        v.push(("Desktop", p.to_path_buf()));
    }
    if let Some(p) = u.document_dir() {
        v.push(("Documents", p.to_path_buf()));
    }
    v.retain(|(_, p)| p.is_dir());
    v
}

impl AutoSortApp {
    /// Draws whichever popup is open. Each popup decides whether it stays open.
    pub(crate) fn modals(&mut self, ctx: &egui::Context) {
        // preview results arrive from a worker thread
        if let Some(rx) = &self.preview_rx {
            if let Ok(entries) = rx.try_recv() {
                if let Some(p) = &mut self.preview {
                    p.entries = Some(entries);
                }
                self.preview_rx = None;
            }
        }
        let Some(state) = self.modal.take() else { return };
        self.modal = match state {
            ModalState::AddFolder(form) => self.add_folder_modal(ctx, form),
            ModalState::EditRule { folder, rule } => self.edit_rule_modal(ctx, folder, rule),
            ModalState::ConfirmRemove(id) => self.confirm_remove_modal(ctx, id),
            ModalState::Preview => self.preview_modal(ctx),
        };
    }

    // ------------------------------------------------------------ add folder

    fn add_folder_modal(&mut self, ctx: &egui::Context, mut form: AddFolderForm) -> Option<ModalState> {
        let mut cancel = false;
        let mut add = false;
        let resp = modal("add_folder").show(ctx, |ui| {
            ui.set_width(520.0);
            title(ui, "Add a folder", "AutoSort watches it and applies your rules automatically.");

            ui.horizontal(|ui| {
                let w = ui.available_width() - 112.0;
                ui.add_sized(
                    [w, 34.0],
                    egui::TextEdit::singleline(&mut form.path)
                        .hint_text("/Users/you/Downloads")
                        .margin(Margin::symmetric(10, 8)),
                );
                if ghost(ui, "Browse…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Choose a folder to watch").pick_folder() {
                        form.path = dir.to_string_lossy().into_owned();
                    }
                }
            });

            let quick = quick_dirs();
            if !quick.is_empty() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new("Quick pick").small().color(t::FAINT));
                    for (name, path) in &quick {
                        let p = path.to_string_lossy().into_owned();
                        if chip(ui, name, form.path == p).clicked() {
                            form.path = p;
                        }
                    }
                });
            }

            let trimmed = form.path.trim();
            let valid = !trimmed.is_empty() && expand_tilde(trimmed).is_dir();
            if !trimmed.is_empty() && !valid {
                ui.label(RichText::new("That folder doesn't exist.").small().color(t::DIM));
            }

            ui.add_space(6.0);
            hairline(ui);
            ui.add_space(2.0);
            setting_row(ui, "Include subfolders", "Also sort files inside folders within it.", &mut form.include_subfolders);
            setting_row(
                ui,
                "Start with sorting rules",
                "Sort by file type, file screenshots, and trash old installers.",
                &mut form.starter_rules,
            );
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(valid, |ui| {
                    if primary(ui, "Add folder").clicked() {
                        add = true;
                    }
                });
                if ghost(ui, "Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if add {
            self.add_folder(form);
            return None;
        }
        if cancel || resp.should_close() {
            return None;
        }
        Some(ModalState::AddFolder(form))
    }

    // ------------------------------------------------------------- edit rule

    fn edit_rule_modal(&mut self, ctx: &egui::Context, folder_id: Uuid, rule_id: Uuid) -> Option<ModalState> {
        let fi = self.config.folders.iter().position(|f| f.id == folder_id)?;
        let ri = self.config.folders[fi].rules.iter().position(|r| r.id == rule_id)?;
        let max_h = (ctx.content_rect().height() - 220.0).max(260.0);
        let (mut delete, mut done) = (false, false);
        let rule = &mut self.config.folders[fi].rules[ri];
        let resp = modal(("edit_rule", rule_id)).show(ctx, |ui| {
            ui.set_width(640.0);
            title(ui, "Edit rule", "Describe which files to match and what to do with them.");
            egui::ScrollArea::vertical().max_height(max_h).auto_shrink([true, true]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                rule_form(ui, rule);
            });
            ui.add_space(12.0);
            hairline(ui);
            ui.add_space(8.0);
            egui::Sides::new().show(
                ui,
                |ui| {
                    if danger(ui, "Delete rule").clicked() {
                        delete = true;
                    }
                },
                |ui| {
                    if primary(ui, "Done").clicked() {
                        done = true;
                    }
                },
            );
        });
        if delete {
            self.config.folders[fi].rules.remove(ri);
            self.toast("Rule deleted", true);
            return None;
        }
        if done || resp.should_close() {
            return None;
        }
        Some(ModalState::EditRule { folder: folder_id, rule: rule_id })
    }

    // -------------------------------------------------------- confirm remove

    fn confirm_remove_modal(&mut self, ctx: &egui::Context, id: Uuid) -> Option<ModalState> {
        let name = self.config.folders.iter().find(|f| f.id == id)?.display_name();
        let (mut remove, mut cancel) = (false, false);
        let resp = modal("confirm_remove").show(ctx, |ui| {
            ui.set_width(420.0);
            title(ui, "Remove this folder?", &format!("AutoSort will stop watching “{name}”. Your files are not touched."));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary(ui, "Keep watching").clicked() {
                    cancel = true;
                }
                if ghost(ui, "Remove").clicked() {
                    remove = true;
                }
            });
        });
        if remove {
            self.config.folders.retain(|f| f.id != id);
            self.page = self.config.folders.first().map(|f| crate::app::Page::Folder(f.id)).unwrap_or(crate::app::Page::Activity);
            self.toast(format!("Stopped watching {name}"), true);
            return None;
        }
        if cancel || resp.should_close() {
            return None;
        }
        Some(ModalState::ConfirmRemove(id))
    }

    // --------------------------------------------------------------- preview

    fn preview_modal(&mut self, ctx: &egui::Context) -> Option<ModalState> {
        let preview = self.preview.as_ref()?;
        let name = preview.folder_name.clone();
        let entries = preview.entries.clone();
        let (mut close, mut run) = (false, false);
        let resp = modal("preview").show(ctx, |ui| {
            ui.set_width(640.0);
            title(ui, &format!("Preview · {name}"), "This is what would happen right now. Nothing has been changed.");
            match &entries {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Checking…").color(t::DIM));
                    });
                }
                Some(list) if list.is_empty() => {
                    ui.label(RichText::new("Nothing would change. (Files modified in the last few seconds, and files already handled, are skipped.)").color(t::DIM));
                }
                Some(list) => {
                    ui.label(RichText::new(format!("{} action{}", list.len(), if list.len() == 1 { "" } else { "s" })).color(t::TEXT));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(300.0).auto_shrink([true, true]).show(ui, |ui| {
                        Frame::new()
                            .fill(t::BG)
                            .stroke(Stroke::new(1.0, t::BORDER))
                            .corner_radius(egui::CornerRadius::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for (i, e) in list.iter().enumerate() {
                                    if i > 0 {
                                        hairline(ui);
                                    }
                                    Frame::new().inner_margin(Margin::symmetric(14, 9)).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 3.0;
                                        let mark = if e.success { "" } else { "  (would fail)" };
                                        ui.add(egui::Label::new(RichText::new(format!("{}{mark}", e.file)).strong()).truncate());
                                        ui.add(egui::Label::new(RichText::new(format!("{}  ·  {}", e.action, e.detail)).small().color(t::DIM)).truncate());
                                    });
                                }
                            });
                    });
                }
            }
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let can_run = entries.as_ref().is_some_and(|l| !l.is_empty());
                ui.add_enabled_ui(can_run, |ui| {
                    if primary(ui, "Run these now").clicked() {
                        run = true;
                    }
                });
                if ghost(ui, "Close").clicked() {
                    close = true;
                }
            });
        });
        if run {
            self.service.run_now();
            self.toast("Running rules…", true);
            close = true;
        }
        if close || resp.should_close() {
            self.preview = None;
            self.preview_rx = None;
            return None;
        }
        Some(ModalState::Preview)
    }
}

// --------------------------------------------------------------------- rule form

fn field_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(t::DIM));
}

fn rule_form(ui: &mut Ui, rule: &mut Rule) {
    ui.spacing_mut().item_spacing = vec2(8.0, 10.0);

    field_label(ui, "Name");
    ui.add_sized([ui.available_width(), 34.0], egui::TextEdit::singleline(&mut rule.name).margin(Margin::symmetric(10, 8)));
    ui.add_space(6.0);

    // ---- when
    section_label(ui, "When");
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Match").color(t::DIM));
            segmented(ui, &mut rule.match_mode, &[(MatchMode::All, "All conditions"), (MatchMode::Any, "Any condition")]);
        });
        let mut remove = None;
        for (i, c) in rule.conditions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                egui::ComboBox::from_id_salt(("attr", c.id)).width(150.0).selected_text(c.attribute.label()).show_ui(ui, |ui| {
                    for a in Attribute::ALL {
                        ui.selectable_value(&mut c.attribute, a, a.label());
                    }
                });
                // keep operator/value valid for the chosen attribute
                if !c.attribute.allowed_operators().contains(&c.op) {
                    c.op = c.attribute.allowed_operators()[0];
                }
                if c.attribute == Attribute::Kind && FileKind::from_key(&c.value).is_none() {
                    c.value = FileKind::Image.key().to_string();
                }
                egui::ComboBox::from_id_salt(("op", c.id)).width(140.0).selected_text(c.op.label()).show_ui(ui, |ui| {
                    for op in c.attribute.allowed_operators() {
                        ui.selectable_value(&mut c.op, *op, op.label());
                    }
                });
                let w = (ui.available_width() - 32.0).max(80.0);
                if c.attribute == Attribute::Kind {
                    let current = FileKind::from_key(&c.value).unwrap_or(FileKind::Image);
                    egui::ComboBox::from_id_salt(("kind", c.id)).width(w - 14.0).selected_text(current.display_name()).show_ui(ui, |ui| {
                        for k in FileKind::ALL {
                            if ui.selectable_label(k == current, k.display_name()).clicked() {
                                c.value = k.key().to_string();
                            }
                        }
                    });
                } else {
                    let hint = match c.attribute {
                        Attribute::Name => "text or regex",
                        Attribute::Extension => "jpg, png, heic",
                        _ => "number",
                    };
                    ui.add_sized([w, 32.0], egui::TextEdit::singleline(&mut c.value).hint_text(hint).margin(Margin::symmetric(10, 7)));
                }
                if quiet(ui, "×").on_hover_text("Remove condition").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            rule.conditions.remove(i);
        }
        if rule.conditions.is_empty() {
            ui.label(RichText::new("Add a condition — a rule with none never matches.").small().color(t::DIM));
        }
        if link(ui, "+  Add condition").clicked() {
            rule.conditions.push(Condition::default());
        }
    });

    // ---- then
    ui.add_space(4.0);
    section_label(ui, "Then");
    card(ui, |ui| {
        let mut remove = None;
        let action_count = rule.actions.len();
        for (i, a) in rule.actions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                egui::ComboBox::from_id_salt(("akind", a.id)).width(210.0).selected_text(a.kind.label()).show_ui(ui, |ui| {
                    for k in ActionKind::ALL {
                        ui.selectable_value(&mut a.kind, k, k.label());
                    }
                });
                if a.kind.uses_parameter() {
                    let w = (ui.available_width() - 32.0).max(80.0);
                    ui.add_sized(
                        [w, 32.0],
                        egui::TextEdit::singleline(&mut a.parameter).hint_text(a.kind.placeholder()).margin(Margin::symmetric(10, 7)),
                    );
                }
                if quiet(ui, "×").on_hover_text("Remove action").clicked() {
                    remove = Some(i);
                }
            });
            if a.kind.uses_conflict_policy() {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new("If a file with that name already exists:").small().color(t::DIM));
                    egui::ComboBox::from_id_salt(("conf", a.id)).width(190.0).selected_text(a.conflict.label()).show_ui(ui, |ui| {
                        for p in ConflictPolicy::ALL {
                            ui.selectable_value(&mut a.conflict, p, p.label());
                        }
                    });
                });
            }
            if i + 1 < action_count {
                hairline(ui);
            }
        }
        if let Some(i) = remove {
            rule.actions.remove(i);
        }
        if rule.actions.is_empty() {
            ui.label(RichText::new("Add an action to say what happens to matching files.").small().color(t::DIM));
        }
        if link(ui, "+  Add action").clicked() {
            rule.actions.push(Action::new(ActionKind::Move, ""));
        }
        ui.label(RichText::new(format!("Tokens you can use in paths and names:  {TOKEN_HELP}")).small().color(t::FAINT));
    });

    ui.add_space(4.0);
    let mut keep_going = !rule.stop_processing;
    if setting_row(ui, "Keep going", "Also run the rules below this one on the same file.", &mut keep_going) {
        rule.stop_processing = !keep_going;
    }
}
