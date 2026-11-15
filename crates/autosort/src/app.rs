use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use autosort_core::store::{self, ActivityLog, InstanceLock, Paths};
use autosort_core::*;
use eframe::egui::{self, Align2, Color32, Frame, Margin, RichText, Stroke, vec2};
use uuid::Uuid;

use crate::notify;
use crate::theme as t;
use crate::tray::{Tray, TrayAction};

pub fn run(paths: Paths, lock: InstanceLock, start_minimized: bool) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AutoSort")
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([820.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AutoSort",
        options,
        Box::new(move |cc| Ok(Box::new(AutoSortApp::new(cc, paths, lock, start_minimized)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Folder(Uuid),
    Activity,
    Settings,
}

#[derive(Default)]
pub struct AddFolderForm {
    pub path: String,
    pub include_subfolders: bool,
    pub starter_rules: bool,
}

pub enum ModalState {
    AddFolder(AddFolderForm),
    EditRule { folder: Uuid, rule: Uuid },
    ConfirmRemove(Uuid),
    Preview,
}

pub struct Preview {
    pub folder_name: String,
    pub entries: Option<Vec<ActivityEntry>>,
}

pub struct Toast {
    text: String,
    ok: bool,
    until: Instant,
}

pub struct AutoSortApp {
    pub(crate) paths: Paths,
    _lock: InstanceLock,
    pub(crate) config: AppConfig,
    shared: Arc<Mutex<AppConfig>>,
    last_saved: String,
    folder_sig: Vec<(Uuid, String, bool, bool)>,
    pub(crate) history: Arc<History>,
    pub(crate) stats: Arc<Stats>,
    pub(crate) activity: Arc<ActivityLog>,
    pub(crate) service: Service,
    pub(crate) page: Page,
    pub(crate) paused: bool,
    /// Ask the service for an immediate pass once this frame's config has been synced.
    pub(crate) pending_sweep: bool,
    pub(crate) modal: Option<ModalState>,
    pub(crate) preview: Option<Preview>,
    pub(crate) preview_rx: Option<Receiver<Vec<ActivityEntry>>>,
    start_minimized: bool,
    pub(crate) launch_at_login: bool,
    toast: Option<Toast>,
    pub(crate) tray: Option<Tray>,
    quitting: bool,
}

fn folder_sig(cfg: &AppConfig) -> Vec<(Uuid, String, bool, bool)> {
    cfg.folders.iter().map(|f| (f.id, f.path.clone(), f.enabled, f.include_subfolders)).collect()
}

pub fn auto_launch() -> Option<auto_launch::AutoLaunch> {
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
        t::apply(&cc.egui_ctx);

        let config: AppConfig = store::load_json(&paths.config()).unwrap_or_default();
        let shared = Arc::new(Mutex::new(config.clone()));
        let history = Arc::new(History::persistent(paths.history()));
        let stats = Arc::new(Stats::persistent(paths.stats()));
        let activity = Arc::new(ActivityLog::new(Some(paths.activity())));

        let sink_log = activity.clone();
        let sink_cfg = shared.clone();
        let ctx = cc.egui_ctx.clone();
        let service = Service::start(
            shared.clone(),
            history.clone(),
            stats.clone(),
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
            stats,
            activity,
            service,
            page,
            paused: false,
            pending_sweep: false,
            modal: None,
            preview: None,
            preview_rx: None,
            start_minimized,
            launch_at_login,
            toast: None,
            tray: Tray::new(&cc.egui_ctx),
            quitting: false,
        }
    }

    pub(crate) fn toast(&mut self, text: impl Into<String>, ok: bool) {
        self.toast = Some(Toast { text: text.into(), ok, until: Instant::now() + Duration::from_secs(4) });
    }

    /// Persists and propagates config edits made by the UI during this frame.
    fn sync_config(&mut self) {
        let json = serde_json::to_string(&self.config).unwrap_or_default();
        if json == self.last_saved {
            return;
        }
        if let Err(e) = store::save_json(&self.paths.config(), &self.config) {
            self.toast(format!("Could not save settings: {e}"), false);
        }
        *self.shared.lock().unwrap() = self.config.clone();
        self.last_saved = json;
        let sig = folder_sig(&self.config);
        if sig != self.folder_sig {
            self.folder_sig = sig;
            self.service.reload();
        }
    }

    /// Adds a folder (and optionally the starter rules) and selects it.
    pub(crate) fn add_folder(&mut self, form: AddFolderForm) {
        let path = form.path.trim().to_string();
        if self.config.folders.iter().any(|f| f.path == path) {
            self.toast("That folder is already being watched.", false);
            return;
        }
        let mut folder = WatchedFolder::new(path);
        folder.include_subfolders = form.include_subfolders;
        if form.starter_rules {
            folder.rules = presets::downloads_starter_rules();
            let learning = self.config.learning_mode;
            folder.rules.iter_mut().for_each(|r| r.approved = !learning);
        }
        let name = folder.display_name();
        self.page = Page::Folder(folder.id);
        self.config.folders.push(folder);
        self.toast(format!("Now watching {name}"), true);
    }

    pub(crate) fn start_preview(&mut self, ctx: &egui::Context, folder: WatchedFolder) {
        let (tx, rx) = mpsc::channel();
        self.preview = Some(Preview { folder_name: folder.display_name(), entries: None });
        self.preview_rx = Some(rx);
        self.modal = Some(ModalState::Preview);
        let engine = Engine::new(self.config.settle_seconds, self.history.clone()).with_stats(self.stats.clone());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(engine.process(&folder, true, chrono::Local::now()));
            ctx.request_repaint();
        });
    }

    pub(crate) fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.service.set_paused(paused);
        if let Some(t) = &self.tray {
            t.set_paused(paused);
        }
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

    /// Dropping a folder on the window opens "Add folder" with it filled in.
    fn handle_drop(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let p = f.path();
            if p.is_dir() {
                self.modal = Some(ModalState::AddFolder(AddFolderForm {
                    path: p.to_string_lossy().into_owned(),
                    ..Default::default()
                }));
                break;
            }
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop_overlay")));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
            painter.rect_stroke(
                screen.shrink(18.0),
                egui::CornerRadius::same(16),
                Stroke::new(2.0, t::ACCENT),
                egui::StrokeKind::Inside,
            );
            painter.text(
                screen.center(),
                Align2::CENTER_CENTER,
                "Drop a folder to watch it",
                egui::FontId::proportional(22.0),
                t::TEXT,
            );
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else { return };
        let now = Instant::now();
        if now >= toast.until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(toast.until - now);
        egui::Area::new(egui::Id::unique("toast"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -24.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(t::SURFACE_HI)
                    .stroke(Stroke::new(1.0, t::BORDER_HI))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(16, 12))
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) })
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal_top(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(8.0, 18.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, if toast.ok { t::ACCENT } else { t::DIM });
                            ui.add(egui::Label::new(RichText::new(&toast.text).color(t::TEXT)).wrap());
                        });
                    });
            });
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

        egui::Panel::left("sidebar")
            .exact_size(244.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(t::SIDEBAR)
                    .stroke(Stroke::new(1.0, t::BORDER))
                    .inner_margin(Margin { left: 14, right: 14, top: 22, bottom: 14 }),
            )
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(t::BG).inner_margin(Margin { left: 40, right: 40, top: 30, bottom: 20 }))
            .show(ui, |ui| match self.page {
                Page::Folder(id) => self.folder_page(ui, id),
                Page::Activity => self.activity_page(ui),
                Page::Settings => self.settings_page(ui),
            });

        self.handle_drop(&ctx);
        self.modals(&ctx);
        self.draw_toast(&ctx);
        self.sync_config();
        if std::mem::take(&mut self.pending_sweep) {
            self.service.sweep();
        }
        // keep the activity list fresh even when no input arrives
        ctx.request_repaint_after(Duration::from_secs(2));
    }
}
