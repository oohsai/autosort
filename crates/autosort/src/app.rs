use std::sync::mpsc::{self, Receiver};

use crate::notify;
use crate::tray::{Tray, TrayAction};
use std::sync::{Arc, Mutex};

use autosort_core::store::{self, ActivityLog, InstanceLock, Paths};
use autosort_core::template::TOKEN_HELP;
use autosort_core::*;
use eframe::egui::{self, Color32, RichText};
use uuid::Uuid;

pub fn run(paths: Paths, lock: InstanceLock, start_minimized: bool) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AutoSort")
            .with_inner_size([1040.0, 700.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AutoSort",
        options,
        Box::new(move |cc| Ok(Box::new(AutoSortApp::new(cc, paths, lock, start_minimized)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Folder(Uuid),
    Activity,
    Settings,
}

enum FolderCmd {
    Preview,
    RunNow,
    Remove,
    AddStarterRules,
}

enum RuleCmd {
    Up,
    Down,
    Duplicate,
    Delete,
}

struct Preview {
    folder_name: String,
    entries: Option<Vec<ActivityEntry>>,
}

pub struct AutoSortApp {
    paths: Paths,
    _lock: InstanceLock,
    config: AppConfig,
    shared: Arc<Mutex<AppConfig>>,
    last_saved: String,
    folder_sig: Vec<(Uuid, String, bool, bool)>,
    history: Arc<History>,
    activity: Arc<ActivityLog>,
    service: Service,
    page: Page,
    paused: bool,
    preview: Option<Preview>,
    preview_rx: Option<Receiver<Vec<ActivityEntry>>>,
    start_minimized: bool,
    launch_at_login: bool,
    notice: Option<(String, bool)>,
    tray: Option<Tray>,
    quitting: bool,
}

fn folder_sig(cfg: &AppConfig) -> Vec<(Uuid, String, bool, bool)> {
    cfg.folders.iter().map(|f| (f.id, f.path.clone(), f.enabled, f.include_subfolders)).collect()
}

fn auto_launch() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name("AutoSort")
        .set_app_path(exe.to_str()?)
        .set_args(&["--minimized"])
        .build()
        .ok()
}

impl AutoSortApp {
    fn new(cc: &eframe::CreationContext<'_>, paths: Paths, lock: InstanceLock, start_minimized: bool) -> Self {
        let config: AppConfig = store::load_json(&paths.config()).unwrap_or_default();
        let shared = Arc::new(Mutex::new(config.clone()));
        let history = Arc::new(History::persistent(paths.history()));
        let activity = Arc::new(ActivityLog::new(Some(paths.activity())));

        let sink_log = activity.clone();
        let sink_cfg = shared.clone();
        let ctx = cc.egui_ctx.clone();
        let service = Service::start(
            shared.clone(),
            history.clone(),
            Arc::new(move |entries| {
                if sink_cfg.lock().unwrap().notifications {
                    notify::summarize(&entries);
                }
                sink_log.push(entries);
                ctx.request_repaint();
            }),
        );

        let page = config.folders.first().map(|f| Page::Folder(f.id)).unwrap_or(Page::Activity);
        let last_saved = serde_json::to_string(&config).unwrap_or_default();
        let launch_at_login = auto_launch().and_then(|a| a.is_enabled().ok()).unwrap_or(false);
        AutoSortApp {
            folder_sig: folder_sig(&config),
            paths,
            _lock: lock,
            config,
            shared,
            last_saved,
            history,
            activity,
            service,
            page,
            paused: false,
            preview: None,
            preview_rx: None,
            start_minimized,
            launch_at_login,
            notice: None,
            tray: Tray::new(&cc.egui_ctx),
            quitting: false,
        }
    }

    /// Persists and propagates config edits made by the UI during this frame.
    fn sync_config(&mut self) {
        let json = serde_json::to_string(&self.config).unwrap_or_default();
        if json == self.last_saved {
            return;
        }
        if let Err(e) = store::save_json(&self.paths.config(), &self.config) {
            self.notice = Some((format!("Could not save settings: {e}"), false));
        }
        *self.shared.lock().unwrap() = self.config.clone();
        self.last_saved = json;
        let sig = folder_sig(&self.config);
        if sig != self.folder_sig {
            self.folder_sig = sig;
            self.service.reload();
        }
    }

    fn add_folder(&mut self, path: String) {
        if self.config.folders.iter().any(|f| f.path == path) {
            self.notice = Some(("That folder is already being watched.".into(), false));
            return;
        }
        let folder = WatchedFolder::new(path);
        self.page = Page::Folder(folder.id);
        self.config.folders.push(folder);
    }

    fn start_preview(&mut self, ctx: &egui::Context, folder: WatchedFolder) {
        let (tx, rx) = mpsc::channel();
        self.preview = Some(Preview { folder_name: folder.display_name(), entries: None });
        self.preview_rx = Some(rx);
        let engine = Engine::new(self.config.settle_seconds, self.history.clone());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(engine.process(&folder, true, chrono::Local::now()));
            ctx.request_repaint();
        });
    }

    // ------------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("AutoSort");
        ui.add_space(6.0);
        ui.label(RichText::new("FOLDERS").small().weak());
        let mut add = false;
        for f in &self.config.folders {
            let text = if f.enabled { f.display_name() } else { format!("{} (off)", f.display_name()) };
            if ui.selectable_label(self.page == Page::Folder(f.id), text).on_hover_text(&f.path).clicked() {
                self.page = Page::Folder(f.id);
            }
        }
        if ui.button("+ Add folder…").clicked() {
            add = true;
        }
        ui.add_space(8.0);
        ui.separator();
        if ui.selectable_label(self.page == Page::Activity, "Activity").clicked() {
            self.page = Page::Activity;
        }
        if ui.selectable_label(self.page == Page::Settings, "Settings").clicked() {
            self.page = Page::Settings;
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(6.0);
            if ui.checkbox(&mut self.paused, "Pause automation").changed() {
                self.service.set_paused(self.paused);
                if let Some(t) = &self.tray {
                    t.set_paused(self.paused);
                }
            }
            if self.paused {
                ui.colored_label(Color32::from_rgb(220, 150, 40), "Paused — no rules are running");
            }
        });
        if add {
            if let Some(dir) = rfd::FileDialog::new().set_title("Choose a folder to watch").pick_folder() {
                self.add_folder(dir.to_string_lossy().into_owned());
            }
        }
    }

    // -------------------------------------------------------------------- pages

    fn folder_page(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let Some(idx) = self.config.folders.iter().position(|f| f.id == id) else {
            self.page = Page::Activity;
            return;
        };
        let mut cmd = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            cmd = folder_editor(ui, &mut self.config.folders[idx]);
        });
        match cmd {
            Some(FolderCmd::Preview) => {
                let f = self.config.folders[idx].clone();
                self.start_preview(&ui.ctx().clone(), f);
            }
            Some(FolderCmd::RunNow) => self.service.run_now(),
            Some(FolderCmd::AddStarterRules) => self.config.folders[idx].rules.extend(presets::downloads_starter_rules()),
            Some(FolderCmd::Remove) => {
                self.config.folders.remove(idx);
                self.page = self.config.folders.first().map(|f| Page::Folder(f.id)).unwrap_or(Page::Activity);
            }
            None => {}
        }
    }

    fn activity_page(&mut self, ui: &mut egui::Ui) {
        let mut add_downloads = false;
        if self.config.folders.is_empty() {
            ui.add_space(20.0);
            ui.heading("Welcome to AutoSort");
            ui.label("Pick a folder to watch, add rules like “if the extension is pdf, move to Documents”, and AutoSort files things for you.");
            ui.add_space(8.0);
            if directories::UserDirs::new().and_then(|d| d.download_dir().map(|p| p.to_path_buf())).is_some() {
                add_downloads = ui.button("Set up my Downloads folder with starter rules").clicked();
            }
            ui.add_space(12.0);
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.heading("Activity");
            if ui.button("Clear").clicked() {
                self.activity.clear();
            }
        });
        let entries = self.activity.snapshot();
        if entries.is_empty() {
            ui.label(RichText::new("Nothing has happened yet.").weak());
        }
        let mut undo_request = None;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            undo_request = entries_grid(ui, "activity_grid", entries.iter().rev().take(500));
        });
        if let Some(id) = undo_request {
            self.notice = Some(match self.activity.undo(id, &self.history) {
                Ok(msg) => (msg, true),
                Err(e) => (format!("Undo failed: {e}"), false),
            });
        }
        if add_downloads {
            if let Some(dir) = directories::UserDirs::new().and_then(|d| d.download_dir().map(|p| p.to_path_buf())) {
                self.add_folder(dir.to_string_lossy().into_owned());
                if let Some(f) = self.config.folders.last_mut() {
                    f.rules = presets::downloads_starter_rules();
                }
            }
        }
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Leave files alone until they haven't changed for");
            ui.add(egui::DragValue::new(&mut self.config.settle_seconds).range(0..=600).suffix(" s"));
        });
        ui.label(RichText::new("Prevents half-downloaded files from being moved.").small().weak());
        ui.add_space(8.0);
        ui.checkbox(&mut self.config.notifications, "Show a notification when files are sorted");
        ui.add_space(8.0);
        if ui.checkbox(&mut self.launch_at_login, "Launch AutoSort at login (minimised)").changed() {
            let result = match auto_launch() {
                Some(a) if self.launch_at_login => a.enable().map_err(|e| e.to_string()),
                Some(a) => a.disable().map_err(|e| e.to_string()),
                None => Err("not supported on this system".into()),
            };
            if let Err(e) = result {
                self.launch_at_login = !self.launch_at_login;
                self.notice = Some((format!("Could not change launch-at-login: {e}"), false));
            }
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label(format!("Settings & history are stored in: {}", self.paths.dir.display()));
        ui.label(RichText::new("Tip: `autosort --headless` runs the same rules with no window; `autosort --once --dry-run` previews from the terminal.").small().weak());
    }

    /// Tray menu actions, and "close button keeps AutoSort running in the background" when a tray exists.
    fn handle_tray_and_close(&mut self, ctx: &egui::Context) {
        let actions = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for a in actions {
            match a {
                TrayAction::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayAction::SetPaused(p) => {
                    self.paused = p;
                    self.service.set_paused(p);
                }
                TrayAction::RunNow => self.service.run_now(),
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        // With a tray icon, the window's close button minimises instead of quitting; use the tray's
        // "Quit AutoSort" to exit. (Minimised rather than hidden so the window can always be restored.)
        if self.tray.is_some() && !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    fn preview_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.preview_rx {
            if let Ok(entries) = rx.try_recv() {
                if let Some(p) = &mut self.preview {
                    p.entries = Some(entries);
                }
                self.preview_rx = None;
            }
        }
        let Some(preview) = &self.preview else { return };
        let mut open = true;
        let mut run = false;
        egui::Window::new(format!("Preview — {}", preview.folder_name))
            .open(&mut open)
            .default_size([640.0, 380.0])
            .show(ctx, |ui| match &preview.entries {
                None => {
                    ui.spinner();
                }
                Some(e) if e.is_empty() => {
                    ui.label("Nothing would change right now. (Files modified in the last few seconds and ones already handled are skipped.)");
                }
                Some(entries) => {
                    ui.label(format!("{} action(s) would run:", entries.len()));
                    egui::ScrollArea::both().max_height(300.0).show(ui, |ui| {
                        let _ = entries_grid(ui, "preview_grid", entries.iter());
                    });
                    if ui.button("Run these now").clicked() {
                        run = true;
                    }
                }
            });
        if run {
            self.service.run_now();
            open = false;
        }
        if !open {
            self.preview = None;
            self.preview_rx = None;
        }
    }
}

impl eframe::App for AutoSortApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.start_minimized {
            self.start_minimized = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        self.handle_tray_and_close(&ctx);

        egui::Panel::left("sidebar").default_size(220.0).resizable(true).show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some((msg, ok)) = self.notice.clone() {
                ui.horizontal(|ui| {
                    let color = if ok { Color32::from_rgb(80, 170, 90) } else { Color32::from_rgb(220, 80, 60) };
                    ui.colored_label(color, msg);
                    if ui.small_button("dismiss").clicked() {
                        self.notice = None;
                    }
                });
            }
            match self.page {
                Page::Folder(id) => self.folder_page(ui, id),
                Page::Activity => self.activity_page(ui),
                Page::Settings => self.settings_page(ui),
            }
        });

        self.preview_window(&ctx);
        self.sync_config();
        // keep the activity list fresh even when no input arrives
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
    }
}

// ----------------------------------------------------------------- components

/// Renders activity rows. Returns the id of an entry whose "Undo" button was clicked.
fn entries_grid<'a>(ui: &mut egui::Ui, id: &str, entries: impl Iterator<Item = &'a ActivityEntry>) -> Option<Uuid> {
    let mut undo = None;
    egui::Grid::new(id).striped(true).num_columns(6).spacing([14.0, 6.0]).show(ui, |ui| {
        for e in entries {
            let (icon, color) = if !e.success {
                ("Failed", Color32::from_rgb(220, 80, 60))
            } else if e.dry_run {
                ("Planned", Color32::GRAY)
            } else {
                ("Done", Color32::from_rgb(80, 170, 90))
            };
            ui.colored_label(color, icon);
            ui.label(e.time.format("%b %d %H:%M").to_string());
            ui.label(RichText::new(&e.file).strong());
            ui.label(format!("{} · {}", e.rule, e.action));
            if e.undone {
                ui.label(RichText::new("undone").weak());
            } else if e.success && !e.dry_run && e.undo.is_some() {
                if ui.small_button("Undo").clicked() {
                    undo = Some(e.id);
                }
            } else {
                ui.label("");
            }
            ui.label(&e.detail);
            ui.end_row();
        }
    });
    undo
}

fn folder_editor(ui: &mut egui::Ui, folder: &mut WatchedFolder) -> Option<FolderCmd> {
    let mut cmd = None;
    ui.add_space(6.0);
    ui.heading(folder.display_name());
    ui.label(RichText::new(&folder.path).weak());
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut folder.enabled, "Enabled");
        ui.checkbox(&mut folder.include_subfolders, "Include subfolders");
        ui.separator();
        if ui.button("Preview").on_hover_text("Show what the rules would do, without changing anything").clicked() {
            cmd = Some(FolderCmd::Preview);
        }
        if ui.button("Run now").clicked() {
            cmd = Some(FolderCmd::RunNow);
        }
        if ui.button("Remove folder").on_hover_text("Stops watching it; your files are not touched").clicked() {
            cmd = Some(FolderCmd::Remove);
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.heading("Rules");
        if ui.button("+ Add rule").clicked() {
            let mut r = Rule::new("New rule");
            r.conditions.push(Condition::default());
            r.actions.push(Action::new(ActionKind::Move, ""));
            folder.rules.push(r);
        }
        if folder.rules.is_empty() && ui.button("Add starter rules").clicked() {
            cmd = Some(FolderCmd::AddStarterRules);
        }
    });
    ui.label(RichText::new("Rules run top to bottom on every file; the first match wins unless “keep going” is ticked.").small().weak());
    ui.add_space(4.0);

    let mut rule_cmd: Option<(usize, RuleCmd)> = None;
    let count = folder.rules.len();
    for (i, rule) in folder.rules.iter_mut().enumerate() {
        let title = if rule.enabled { rule.name.clone() } else { format!("{} (off)", rule.name) };
        egui::CollapsingHeader::new(RichText::new(format!("{}. {}", i + 1, title)).strong())
            .id_salt(rule.id)
            .default_open(false)
            .show(ui, |ui| {
                if let Some(c) = rule_editor(ui, rule, i, count) {
                    rule_cmd = Some((i, c));
                }
            });
    }
    if let Some((i, c)) = rule_cmd {
        match c {
            RuleCmd::Up if i > 0 => folder.rules.swap(i, i - 1),
            RuleCmd::Down if i + 1 < folder.rules.len() => folder.rules.swap(i, i + 1),
            RuleCmd::Duplicate => {
                let mut copy = folder.rules[i].clone();
                copy.id = Uuid::new_v4();
                copy.name = format!("{} copy", copy.name);
                folder.rules.insert(i + 1, copy);
            }
            RuleCmd::Delete => {
                folder.rules.remove(i);
            }
            _ => {}
        }
    }
    cmd
}

fn rule_editor(ui: &mut egui::Ui, rule: &mut Rule, index: usize, count: usize) -> Option<RuleCmd> {
    let mut cmd = None;
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut rule.name).desired_width(240.0));
        ui.checkbox(&mut rule.enabled, "Enabled");
        if ui.add_enabled(index > 0, egui::Button::new("Up")).clicked() {
            cmd = Some(RuleCmd::Up);
        }
        if ui.add_enabled(index + 1 < count, egui::Button::new("Down")).clicked() {
            cmd = Some(RuleCmd::Down);
        }
        if ui.button("Duplicate").clicked() {
            cmd = Some(RuleCmd::Duplicate);
        }
        if ui.button("Delete").clicked() {
            cmd = Some(RuleCmd::Delete);
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("IF").strong());
        egui::ComboBox::from_id_salt(("mode", rule.id)).selected_text(rule.match_mode.label()).show_ui(ui, |ui| {
            for m in MatchMode::ALL {
                ui.selectable_value(&mut rule.match_mode, m, m.label());
            }
        });
        ui.label("match:");
    });
    let mut remove_condition = None;
    for (ci, c) in rule.conditions.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
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
            if c.attribute == Attribute::Kind {
                let current = FileKind::from_key(&c.value).unwrap_or(FileKind::Image);
                egui::ComboBox::from_id_salt(("kind", c.id)).selected_text(current.display_name()).show_ui(ui, |ui| {
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
                ui.add(egui::TextEdit::singleline(&mut c.value).hint_text(hint).desired_width(200.0));
            }
            if ui.button("Remove").on_hover_text("Remove condition").clicked() {
                remove_condition = Some(ci);
            }
        });
    }
    if let Some(ci) = remove_condition {
        rule.conditions.remove(ci);
    }
    if ui.small_button("+ condition").clicked() {
        rule.conditions.push(Condition::default());
    }
    if rule.conditions.is_empty() {
        ui.colored_label(Color32::from_rgb(220, 150, 40), "A rule with no conditions never matches.");
    }

    ui.add_space(6.0);
    ui.label(RichText::new("THEN").strong());
    let mut remove_action = None;
    for (ai, a) in rule.actions.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt(("akind", a.id)).width(190.0).selected_text(a.kind.label()).show_ui(ui, |ui| {
                for k in ActionKind::ALL {
                    ui.selectable_value(&mut a.kind, k, k.label());
                }
            });
            if a.kind.uses_parameter() {
                ui.add(egui::TextEdit::singleline(&mut a.parameter).hint_text(a.kind.placeholder()).desired_width(260.0));
            }
            if a.kind.uses_conflict_policy() {
                ui.label("if exists:");
                egui::ComboBox::from_id_salt(("conf", a.id)).selected_text(a.conflict.label()).show_ui(ui, |ui| {
                    for p in ConflictPolicy::ALL {
                        ui.selectable_value(&mut a.conflict, p, p.label());
                    }
                });
            }
            if ui.button("Remove").on_hover_text("Remove action").clicked() {
                remove_action = Some(ai);
            }
        });
    }
    if let Some(ai) = remove_action {
        rule.actions.remove(ai);
    }
    if ui.small_button("+ action").clicked() {
        rule.actions.push(Action::new(ActionKind::Move, ""));
    }
    ui.label(RichText::new(format!("Tokens: {TOKEN_HELP}")).small().weak());

    ui.add_space(4.0);
    let mut keep_going = !rule.stop_processing;
    if ui.checkbox(&mut keep_going, "Keep going: also run later rules on this file").changed() {
        rule.stop_processing = !keep_going;
    }
    ui.add_space(6.0);
    cmd
}
