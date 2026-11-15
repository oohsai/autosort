//! The three pages (folder, activity, settings) and the sidebar.

use autosort_core::template::TOKEN_HELP;
use autosort_core::*;
use eframe::egui::{self, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};
use uuid::Uuid;

use crate::app::{AddFolderForm, AutoSortApp, ModalState, Page, auto_launch};
use crate::theme as t;
use crate::widgets::*;

enum FolderCmd {
    Preview,
    RunNow,
    Remove,
    StarterRules,
    NewRule,
    Edit(Uuid),
    Approve(Uuid),
    Export,
    Import,
    Rule(usize, RuleCmd),
}

#[derive(Clone, Copy)]
enum RuleCmd {
    Up,
    Down,
    Duplicate,
    Delete,
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(26.0).strong().color(t::TEXT));
}

fn page_title_row(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    egui::Sides::new().height(40.0).show(ui, |ui| heading(ui, title), right);
}

impl AutoSortApp {
    // ---------------------------------------------------------------- sidebar

    pub(crate) fn sidebar(&mut self, ui: &mut Ui) {
        // bottom block first so the folder list can take whatever space is left
        egui::Panel::bottom("sidebar_bottom").frame(Frame::NONE).show_separator_line(false).show(ui, |ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            if nav_item(ui, self.page == Page::Activity, Icon::Activity, "Activity", None).clicked() {
                self.page = Page::Activity;
            }
            if nav_item(ui, self.page == Page::Settings, Icon::Sliders, "Settings", None).clicked() {
                self.page = Page::Settings;
            }
            ui.add_space(8.0);
            hairline(ui);
            ui.add_space(6.0);
            let mut running = !self.paused;
            let is_running = running;
            egui::Sides::new().height(28.0).show(
                ui,
                |ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(if is_running { "Automation on" } else { "Paused" }).color(if is_running { t::TEXT } else { t::DIM }));
                },
                |ui| {
                    ui.add_space(6.0);
                    if toggle(ui, &mut running).changed() {
                        self.set_paused(!running);
                    }
                },
            );
        });

        // wordmark
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            ui.painter().rect_filled(rect, egui::CornerRadius::same(7), t::ACCENT);
            for (i, w) in [12.0, 8.5, 5.0].into_iter().enumerate() {
                let y = rect.top() + 7.0 + i as f32 * 5.0;
                ui.painter().line_segment(
                    [egui::pos2(rect.left() + 6.0, y), egui::pos2(rect.left() + 6.0 + w, y)],
                    Stroke::new(1.8, t::ACCENT_ON),
                );
            }
            ui.add_space(2.0);
            ui.label(RichText::new("AutoSort").size(18.0).strong().color(t::TEXT));
        });
        ui.add_space(26.0);

        ui.horizontal(|ui| {
            ui.add_space(6.0);
            section_label(ui, "Folders");
        });
        ui.add_space(2.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        let mut select = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for f in &self.config.folders {
                let r = nav_item(ui, self.page == Page::Folder(f.id), Icon::Folder, &f.display_name(), Some(f.enabled));
                if r.on_hover_text(&f.path).clicked() {
                    select = Some(f.id);
                }
            }
            ui.add_space(6.0);
            if link(ui, "+  Add folder").clicked() {
                self.modal = Some(ModalState::AddFolder(AddFolderForm { starter_rules: true, ..Default::default() }));
            }
        });
        if let Some(id) = select {
            self.page = Page::Folder(id);
        }
    }

    // ------------------------------------------------------------ folder page

    pub(crate) fn folder_page(&mut self, ui: &mut Ui, id: Uuid) {
        let Some(idx) = self.config.folders.iter().position(|f| f.id == id) else {
            self.page = Page::Activity;
            return;
        };
        let mut cmd: Option<FolderCmd> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(880.0));
            let stats = self.stats.clone();
            let folder = &mut self.config.folders[idx];
            let has_rules = !folder.rules.is_empty();

            // header
            egui::Sides::new().height(54.0).shrink_left().truncate().show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        heading(ui, &folder.display_name());
                        ui.add(egui::Label::new(RichText::new(&folder.path).small().color(t::DIM)).truncate());
                    });
                },
                |ui| {
                    if primary(ui, "Run now").clicked() {
                        cmd = Some(FolderCmd::RunNow);
                    }
                    if ghost(ui, "Preview").on_hover_text("See what the rules would do — nothing is changed").clicked() {
                        cmd = Some(FolderCmd::Preview);
                    }
                },
            );
            ui.add_space(8.0);

            // options
            card(ui, |ui| {
                setting_row(ui, "Watching", "Apply the rules to files in this folder.", &mut folder.enabled);
                ui.add_space(4.0);
                hairline(ui);
                ui.add_space(4.0);
                setting_row(ui, "Include subfolders", "Also sort files inside folders within this one.", &mut folder.include_subfolders);
                ui.add_space(4.0);
                hairline(ui);
                ui.add_space(2.0);
                ignore_editor(ui, folder);
            });

            // rules
            ui.add_space(14.0);
            egui::Sides::new().height(24.0).show(
                ui,
                |ui| section_label(ui, "Rules"),
                |ui| {
                    if link(ui, "+  New rule").clicked() {
                        cmd = Some(FolderCmd::NewRule);
                    }
                    ui.menu_button(RichText::new("···").color(t::DIM), |ui| {
                        if ui.button("Import rules…").clicked() {
                            cmd = Some(FolderCmd::Import);
                            ui.close();
                        }
                        if ui.add_enabled(has_rules, egui::Button::new("Export rules…")).clicked() {
                            cmd = Some(FolderCmd::Export);
                            ui.close();
                        }
                    });
                },
            );
            ui.add_space(2.0);

            if folder.rules.is_empty() {
                card(ui, |ui| {
                    empty_state(
                        ui,
                        "No rules yet",
                        "Rules tell AutoSort what to do with files in this folder.",
                        256.0,
                        |ui| {
                            if primary(ui, "Add starter rules").clicked() {
                                cmd = Some(FolderCmd::StarterRules);
                            }
                            if ghost(ui, "New rule").clicked() {
                                cmd = Some(FolderCmd::NewRule);
                            }
                        },
                    );
                    ui.add_space(24.0);
                });
            }
            let count = folder.rules.len();
            for (i, rule) in folder.rules.iter_mut().enumerate() {
                let (rid, name, summary, was_enabled) = (rule.id, rule.name.clone(), rule.summary(), rule.enabled);
                let (approved, schedule) = (rule.approved, rule.schedule);
                let st = stats.get(rid);
                card(ui, |ui| {
                    egui::Sides::new().height(58.0).shrink_left().truncate().show(
                        ui,
                        |ui| {
                            toggle(ui, &mut rule.enabled);
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let name_color = if was_enabled { t::TEXT } else { t::DIM };
                                    ui.label(RichText::new(&name).strong().color(name_color));
                                    if !approved {
                                        badge(ui, "Preview only", t::TEXT, t::SURFACE_HI, Stroke::new(1.0, t::BORDER_HI));
                                    }
                                    if schedule != Schedule::Always {
                                        badge(ui, &schedule.summary(), t::ACCENT, t::ACCENT_SOFT, Stroke::new(1.0, t::ACCENT.gamma_multiply(0.5)));
                                    }
                                });
                                ui.add(egui::Label::new(RichText::new(&summary).small().color(t::DIM)).truncate());
                                let usage = match st.last_match {
                                    Some(last) if st.files > 0 => {
                                        format!("{} file{} handled  ·  last {}", st.files, if st.files == 1 { "" } else { "s" }, fmt_time(last))
                                    }
                                    _ if !approved => "Waiting for your approval — nothing has been changed yet".to_string(),
                                    _ => "Hasn't handled any files yet".to_string(),
                                };
                                ui.add(egui::Label::new(RichText::new(usage).small().color(t::FAINT)).truncate());
                            });
                        },
                        |ui| {
                            ui.menu_button(RichText::new("···").color(t::DIM), |ui| {
                                ui.set_min_width(130.0);
                                if ui.add_enabled(i > 0, egui::Button::new("Move up")).clicked() {
                                    cmd = Some(FolderCmd::Rule(i, RuleCmd::Up));
                                    ui.close();
                                }
                                if ui.add_enabled(i + 1 < count, egui::Button::new("Move down")).clicked() {
                                    cmd = Some(FolderCmd::Rule(i, RuleCmd::Down));
                                    ui.close();
                                }
                                if ui.button("Duplicate").clicked() {
                                    cmd = Some(FolderCmd::Rule(i, RuleCmd::Duplicate));
                                    ui.close();
                                }
                                if ui.button("Delete").clicked() {
                                    cmd = Some(FolderCmd::Rule(i, RuleCmd::Delete));
                                    ui.close();
                                }
                            });
                            if ghost(ui, "Edit").clicked() {
                                cmd = Some(FolderCmd::Edit(rid));
                            }
                            if !approved && primary(ui, "Approve").on_hover_text("Let this rule start changing files").clicked() {
                                cmd = Some(FolderCmd::Approve(rid));
                            }
                        },
                    );
                });
            }
            if !folder.rules.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new("Rules run top to bottom. The first match wins unless “Keep going” is on.").small().color(t::FAINT));
            }

            ui.add_space(26.0);
            if danger(ui, "Remove this folder").clicked() {
                cmd = Some(FolderCmd::Remove);
            }
            ui.add_space(20.0);
        });

        match cmd {
            Some(FolderCmd::Preview) => {
                let f = self.config.folders[idx].clone();
                self.start_preview(&ui.ctx().clone(), f);
            }
            Some(FolderCmd::RunNow) => {
                self.service.run_now();
                self.toast("Running rules…", true);
            }
            Some(FolderCmd::Remove) => self.modal = Some(ModalState::ConfirmRemove(id)),
            Some(FolderCmd::StarterRules) => {
                let learning = self.config.learning_mode;
                let mut rules = presets::downloads_starter_rules();
                rules.iter_mut().for_each(|r| r.approved = !learning);
                self.config.folders[idx].rules.extend(rules);
                if learning {
                    self.toast("Starter rules added — Preview them, then Approve", true);
                }
            }
            Some(FolderCmd::Approve(rid)) => {
                if let Some(r) = self.config.folders[idx].rules.iter_mut().find(|r| r.id == rid) {
                    r.approved = true;
                }
                self.pending_sweep = true;
                self.toast("Rule approved — it will run from now on", true);
            }
            Some(FolderCmd::Export) => {
                let rules = self.config.folders[idx].rules.clone();
                let name = format!("{}-rules.json", self.config.folders[idx].display_name().to_lowercase().replace(' ', "-"));
                if let Some(path) = rfd::FileDialog::new().set_title("Export rules").set_file_name(&name).add_filter("AutoSort rules", &["json"]).save_file() {
                    match std::fs::write(&path, rules_io::export_rules(&rules)) {
                        Ok(()) => self.toast(format!("Exported {} rule{}", rules.len(), if rules.len() == 1 { "" } else { "s" }), true),
                        Err(e) => self.toast(format!("Could not save: {e}"), false),
                    }
                }
            }
            Some(FolderCmd::Import) => {
                if let Some(path) = rfd::FileDialog::new().set_title("Import rules").add_filter("AutoSort rules", &["json"]).pick_file() {
                    let learning = self.config.learning_mode;
                    match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| rules_io::import_rules(&t, learning)) {
                        Ok(rules) => {
                            let n = rules.len();
                            self.config.folders[idx].rules.extend(rules);
                            let tail = if learning && n > 0 { " — Preview and approve them to start" } else { "" };
                            self.toast(format!("Imported {n} rule{}{tail}", if n == 1 { "" } else { "s" }), true);
                        }
                        Err(e) => self.toast(format!("Could not import: {e}"), false),
                    }
                }
            }
            Some(FolderCmd::NewRule) => {
                let mut r = Rule::new("New rule");
                r.approved = !self.config.learning_mode;
                r.conditions.push(Condition::default());
                r.actions.push(Action::new(ActionKind::Move, ""));
                let rid = r.id;
                self.config.folders[idx].rules.push(r);
                self.modal = Some(ModalState::EditRule { folder: id, rule: rid });
            }
            Some(FolderCmd::Edit(rid)) => self.modal = Some(ModalState::EditRule { folder: id, rule: rid }),
            Some(FolderCmd::Rule(i, c)) => {
                let rules = &mut self.config.folders[idx].rules;
                match c {
                    RuleCmd::Up if i > 0 => rules.swap(i, i - 1),
                    RuleCmd::Down if i + 1 < rules.len() => rules.swap(i, i + 1),
                    RuleCmd::Duplicate => {
                        let mut copy = rules[i].clone();
                        copy.id = Uuid::new_v4();
                        copy.name = format!("{} copy", copy.name);
                        rules.insert(i + 1, copy);
                    }
                    RuleCmd::Delete => {
                        rules.remove(i);
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }

    // ----------------------------------------------------------- activity page

    pub(crate) fn activity_page(&mut self, ui: &mut Ui) {
        ui.set_max_width(ui.available_width().min(880.0));
        let mut clear = false;
        let has_entries = !self.activity.is_empty();
        page_title_row(ui, "Activity", |ui| {
            if has_entries && quiet(ui, "Clear").clicked() {
                clear = true;
            }
        });
        if clear {
            self.activity.clear();
        }
        ui.add_space(8.0);

        if self.config.folders.is_empty() {
            card(ui, |ui| {
                empty_state(
                    ui,
                    "Welcome to AutoSort",
                    "Pick a folder to watch and AutoSort will file things for you.",
                    120.0,
                    |ui| {
                        if primary(ui, "Add a folder").clicked() {
                            self.modal = Some(ModalState::AddFolder(AddFolderForm { starter_rules: true, ..Default::default() }));
                        }
                    },
                );
                ui.add_space(24.0);
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Tip: you can also drag a folder onto this window.").small().color(t::FAINT));
            return;
        }

        let entries = self.activity.snapshot();
        if entries.is_empty() {
            card(ui, |ui| {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Nothing yet").size(16.0).color(t::TEXT));
                    ui.label(RichText::new("When AutoSort moves, renames or copies a file, it shows up here — with an Undo.").color(t::DIM));
                });
                ui.add_space(20.0);
            });
            return;
        }

        let mut undo_request = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::new()
                .fill(t::SURFACE)
                .stroke(Stroke::new(1.0, t::BORDER))
                .corner_radius(egui::CornerRadius::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, e) in entries.iter().rev().take(300).enumerate() {
                        if i > 0 {
                            hairline(ui);
                        }
                        if let Some(id) = activity_row(ui, e) {
                            undo_request = Some(id);
                        }
                    }
                });
            ui.add_space(12.0);
        });
        if let Some(id) = undo_request {
            match self.activity.undo(id, &self.history) {
                Ok(msg) => self.toast(msg, true),
                Err(e) => self.toast(format!("Undo failed: {e}"), false),
            }
        }
    }

    // ----------------------------------------------------------- settings page

    pub(crate) fn settings_page(&mut self, ui: &mut Ui) {
        ui.set_max_width(ui.available_width().min(720.0));
        page_title_row(ui, "Settings", |_| {});
        ui.add_space(8.0);
        card(ui, |ui| {
            setting_row(ui, "Notifications", "Show a notification when files are sorted.", &mut self.config.notifications);
            ui.add_space(4.0);
            hairline(ui);
            ui.add_space(4.0);
            setting_row(
                ui,
                "Learning mode",
                "New, imported and starter rules only preview until you approve them.",
                &mut self.config.learning_mode,
            );
            ui.add_space(4.0);
            hairline(ui);
            ui.add_space(4.0);
            let mut launch = self.launch_at_login;
            if setting_row(ui, "Launch at login", "Start AutoSort minimised when you sign in.", &mut launch) {
                let result = match auto_launch() {
                    Some(a) if launch => a.enable().map_err(|e| e.to_string()),
                    Some(a) => a.disable().map_err(|e| e.to_string()),
                    None => Err("not supported on this system".into()),
                };
                match result {
                    Ok(()) => self.launch_at_login = launch,
                    Err(e) => self.toast(format!("Could not change launch at login: {e}"), false),
                }
            }
            ui.add_space(4.0);
            hairline(ui);
            ui.add_space(4.0);
            egui::Sides::new().show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label("Wait before touching new files");
                        ui.label(RichText::new("Prevents half-downloaded files from being moved.").small().color(t::DIM));
                    });
                },
                |ui| {
                    ui.add(egui::DragValue::new(&mut self.config.settle_seconds).range(0..=600).suffix(" s"));
                },
            );
        });

        ui.add_space(14.0);
        section_label(ui, "File manager");
        ui.add_space(2.0);
        card(ui, |ui| {
            let installed = autosort_core::integration::is_installed();
            let mut action = None;
            egui::Sides::new().show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label("Right-click “Sort with AutoSort”");
                        let hint = if installed { "Installed — right-click any file or folder in a watched folder." } else { "Sort files from Finder or Explorer without opening AutoSort." };
                        ui.label(RichText::new(hint).small().color(t::DIM));
                    });
                },
                |ui| {
                    if installed {
                        if ghost(ui, "Remove").clicked() {
                            action = Some(false);
                        }
                    } else if primary(ui, "Install").clicked() {
                        action = Some(true);
                    }
                },
            );
            match action {
                Some(true) => match std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| autosort_core::integration::install(&exe)) {
                    Ok(msg) => self.toast(msg, true),
                    Err(e) => self.toast(format!("Could not install: {e}"), false),
                },
                Some(false) => match autosort_core::integration::uninstall() {
                    Ok(msg) => self.toast(msg, true),
                    Err(e) => self.toast(format!("Could not remove: {e}"), false),
                },
                None => {}
            }
        });

        ui.add_space(14.0);
        section_label(ui, "About");
        ui.add_space(2.0);
        card(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(RichText::new(format!("AutoSort {}", env!("CARGO_PKG_VERSION"))).strong());
            ui.label(RichText::new(format!("Settings and history live in {}", self.paths.dir.display())).small().color(t::DIM));
            ui.label(RichText::new("Tokens for folders and names:").small().color(t::DIM));
            ui.label(RichText::new(TOKEN_HELP).small().monospace().color(t::DIM));
            ui.label(RichText::new("Run without a window:  autosort --headless    Preview from a terminal:  autosort --once --dry-run").small().color(t::FAINT));
        });
    }
}

/// Collapsible multi-line editor for the folder's ignore patterns (one per line).
fn ignore_editor(ui: &mut Ui, folder: &mut WatchedFolder) {
    let id = egui::Id::unique(("ignore_buf", folder.id));
    let count = folder.ignore.len();
    let title = if count == 0 { "Ignore files".to_string() } else { format!("Ignore files ({count})") };
    egui::CollapsingHeader::new(RichText::new(title).color(t::TEXT))
        .id_salt(("ignore_hdr", folder.id))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new("Rules never touch matching files. One pattern per line:  *.psd   draft*   Archive/   photos/raw/*").small().color(t::DIM));
            let mut buf: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| folder.ignore.join("\n"));
            let changed = ui
                .add(
                    egui::TextEdit::multiline(&mut buf)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("*.psd")
                        .margin(Margin::symmetric(10, 8)),
                )
                .changed();
            if changed {
                folder.ignore = buf.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
            }
            ui.data_mut(|d| d.insert_temp(id, buf));
        });
}

// ----------------------------------------------------------------- activity row

fn fmt_time(time: chrono::DateTime<chrono::Local>) -> String {
    let now = chrono::Local::now();
    if time.date_naive() == now.date_naive() {
        format!("Today {}", time.format("%H:%M"))
    } else {
        time.format("%b %d  %H:%M").to_string()
    }
}

/// One entry. Returns the id if its "Undo" was clicked.
fn activity_row(ui: &mut Ui, e: &ActivityEntry) -> Option<Uuid> {
    let mut undo = None;
    Frame::new().inner_margin(Margin::symmetric(16, 11)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.0;
        egui::Sides::new().height(30.0).shrink_left().truncate().show(
            ui,
            |ui| {
                if e.undone {
                    badge(ui, "Undone", t::FAINT, egui::Color32::TRANSPARENT, Stroke::new(1.0, t::BORDER));
                } else if !e.success {
                    badge(ui, "Failed", t::TEXT, t::SURFACE_HI, Stroke::new(1.0, t::BORDER_HI));
                } else if e.dry_run {
                    badge(ui, "Preview", t::DIM, egui::Color32::TRANSPARENT, Stroke::new(1.0, t::BORDER_HI));
                } else {
                    badge(ui, "Done", t::ACCENT, t::ACCENT_SOFT, Stroke::new(1.0, t::ACCENT.gamma_multiply(0.5)));
                }
                ui.add(egui::Label::new(RichText::new(&e.file).strong().color(if e.undone { t::DIM } else { t::TEXT })).truncate());
                ui.add(egui::Label::new(RichText::new(format!("{}  ·  {}", e.rule, e.action)).small().color(t::DIM)).truncate());
            },
            |ui| {
                if !e.undone && e.success && !e.dry_run && e.undo.is_some() && link(ui, "Undo").clicked() {
                    undo = Some(e.id);
                }
                ui.label(RichText::new(fmt_time(e.time)).small().color(t::FAINT));
            },
        );
        ui.add(egui::Label::new(RichText::new(&e.detail).small().color(t::DIM)).truncate()).on_hover_text(&e.detail);
    });
    undo
}
