//! AutoSort — rule-based automatic file organiser.
//!
//! * `autosort`                     open the desktop app (rules run in the background while it is open)
//! * `autosort --minimized`         same, but start minimised (used for "launch at login")
//! * `autosort --headless`          no window: watch folders and apply rules (servers, services)
//! * `autosort --once [--dry-run]`  apply rules to existing files once and exit
//! * `autosort --sort PATH...`      sort these files/folders now (what the right-click action runs)

mod app;
mod modals;
mod notify;
mod theme;
mod tray;
mod views;
mod widgets;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use autosort_core::store::{self, ActivityLog, InstanceLock, Paths};
use autosort_core::{AppConfig, Engine, History, RunOptions, Service, Stats, integration, sorting};

const HELP: &str = "AutoSort — rule-based automatic file organiser

USAGE:
    autosort [OPTIONS]
    autosort --sort PATH...

OPTIONS:
    --headless                Run without a window: watch folders and apply rules until stopped
    --once                    Apply rules to the files currently in the watched folders, then exit
    --dry-run                 With --once: only print what would happen, change nothing
    --sort PATH...            Sort these files or folders now, using the rules of the watched folder
                              they are in (this is what the right-click action runs)
    --install-integration     Add \"Sort with AutoSort\" to your file manager's right-click menu
    --uninstall-integration   Remove it again
    --minimized               Start the desktop app minimised
    -h, --help                Show this help

Config lives in the OS config dir (override with AUTOSORT_HOME).";

const FLAGS: [&str; 6] = ["--headless", "--once", "--dry-run", "--minimized", "--install-integration", "--uninstall-integration"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("-h") || has("--help") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let paths = Paths::discover();

    // `--sort` swallows everything after it as paths.
    if let Some(pos) = args.iter().position(|a| a == "--sort") {
        let targets: Vec<PathBuf> = args[pos + 1..].iter().map(PathBuf::from).collect();
        return run_sort(&paths, targets);
    }
    if let Some(bad) = args.iter().find(|a| !FLAGS.contains(&a.as_str())) {
        eprintln!("autosort: unknown option '{bad}'\n\n{HELP}");
        return ExitCode::from(2);
    }
    if has("--dry-run") && !has("--once") {
        eprintln!("autosort: --dry-run only makes sense together with --once");
        return ExitCode::from(2);
    }
    if has("--install-integration") || has("--uninstall-integration") {
        let result = if has("--install-integration") {
            std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| integration::install(&exe))
        } else {
            integration::uninstall()
        };
        return match result {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("autosort: {e}");
                ExitCode::from(1)
            }
        };
    }
    if has("--once") {
        return run_once(&paths, has("--dry-run"));
    }

    // Everything below actually changes files in the background, so only one instance may run.
    let Some(lock) = InstanceLock::acquire(&paths) else {
        eprintln!("autosort: another AutoSort is already running (lock: {})", paths.lock().display());
        return ExitCode::from(1);
    };
    if has("--headless") {
        return run_headless(&paths, lock);
    }
    match app::run(paths, lock, has("--minimized")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("autosort: {e}");
            ExitCode::from(1)
        }
    }
}

fn load_config(paths: &Paths) -> AppConfig {
    store::load_json(&paths.config()).unwrap_or_default()
}

fn print_entry(e: &autosort_core::ActivityEntry) {
    let mark = if !e.success { "FAIL" } else if e.dry_run { "PLAN" } else { " OK " };
    println!("[{mark}] {} — {} · {} — {}", e.file, e.rule, e.action, e.detail);
}

fn engine(paths: &Paths, settle: u64) -> Engine {
    Engine::new(settle, Arc::new(History::persistent(paths.history()))).with_stats(Arc::new(Stats::persistent(paths.stats())))
}

fn run_once(paths: &Paths, dry_run: bool) -> ExitCode {
    let _lock = if dry_run {
        None
    } else {
        match InstanceLock::acquire(paths) {
            Some(l) => Some(l),
            None => {
                eprintln!("autosort: another AutoSort is running; stop it first or use --dry-run");
                return ExitCode::from(1);
            }
        }
    };
    let cfg = load_config(paths);
    let engine = engine(paths, cfg.settle_seconds);
    let log = ActivityLog::new(Some(paths.activity()));
    let (mut total, mut failed) = (0, 0);
    for folder in cfg.folders.iter().filter(|f| f.enabled) {
        let entries = engine.process(folder, dry_run, chrono::Local::now());
        for e in &entries {
            print_entry(e);
        }
        total += entries.len();
        failed += entries.iter().filter(|e| !e.success).count();
        if !dry_run {
            log.push(entries);
        }
    }
    println!("{total} action(s){}, {failed} failed", if dry_run { " planned" } else { "" });
    if failed > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

/// `autosort --sort PATH...` — explicit requests ignore "already handled" memory and the settle delay.
fn run_sort(paths: &Paths, targets: Vec<PathBuf>) -> ExitCode {
    if targets.is_empty() {
        eprintln!("autosort: --sort needs at least one file or folder");
        return ExitCode::from(2);
    }
    let cfg = load_config(paths);
    let (plans, rejected) = sorting::plan_sort(&cfg, &targets);
    for (p, why) in &rejected {
        eprintln!("autosort: {} {why}", p.display());
    }
    let engine = engine(paths, 0);
    let log = ActivityLog::new(Some(paths.activity()));
    let mut all = Vec::new();
    let mut awaiting = false;
    for plan in plans {
        awaiting |= plan.folder.rules.iter().any(|r| r.enabled && !r.approved);
        let opts = RunOptions { only: Some(plan.files), ignore_history: true, ignore_settle: true, force_scheduled: true, ..Default::default() };
        let entries = engine.process_with(&plan.folder, &opts, chrono::Local::now());
        for e in &entries {
            print_entry(e);
        }
        log.push(entries.clone());
        all.extend(entries);
    }
    let failed = all.iter().filter(|e| !e.success).count();
    if all.is_empty() && rejected.is_empty() {
        let hint = if awaiting { " Some rules are still awaiting approval in AutoSort." } else { "" };
        println!("No rule matched.{hint}");
    }
    if cfg.notifications {
        if !all.is_empty() {
            notify::summarize(&all).join().ok();
        } else if !rejected.is_empty() {
            notify::message("AutoSort", "That item isn't inside a watched folder. Add its folder in AutoSort first.").join().ok();
        } else {
            notify::message("AutoSort", "No rule matched.").join().ok();
        }
    }
    if failed > 0 || !rejected.is_empty() { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn run_headless(paths: &Paths, _lock: InstanceLock) -> ExitCode {
    let cfg = load_config(paths);
    if cfg.folders.is_empty() {
        eprintln!("autosort: no folders configured yet — open the app once to set them up");
    }
    let log = Arc::new(ActivityLog::new(Some(paths.activity())));
    let sink_log = log.clone();
    let shared = Arc::new(Mutex::new(cfg));
    let sink_cfg = shared.clone();
    let _service = Service::start(
        shared,
        Arc::new(History::persistent(paths.history())),
        Arc::new(Stats::persistent(paths.stats())),
        Arc::new(move |entries| {
            for e in &entries {
                print_entry(e);
            }
            if sink_cfg.lock().unwrap().notifications {
                notify::summarize(&entries);
            }
            sink_log.push(entries);
        }),
    );
    println!("autosort: watching folders (Ctrl+C to stop)");
    loop {
        std::thread::park();
    }
}
