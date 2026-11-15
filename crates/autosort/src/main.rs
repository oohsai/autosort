//! AutoSort — rule-based automatic file organiser.
//!
//! * `autosort`              open the desktop app (rules run in the background while it is open)
//! * `autosort --minimized`  same, but start minimised (used for "launch at login")
//! * `autosort --headless`   no window: watch folders and apply rules (servers, services)
//! * `autosort --once`       apply rules to existing files once and exit (`--dry-run` to preview)

mod app;
mod notify;
mod tray;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use autosort_core::store::{self, ActivityLog, InstanceLock, Paths};
use autosort_core::{AppConfig, Engine, History, Service};

const HELP: &str = "AutoSort — rule-based automatic file organiser

USAGE:
    autosort [OPTIONS]

OPTIONS:
    --headless    Run without a window: watch folders and apply rules until stopped
    --once        Apply rules to the files currently in the watched folders, then exit
    --dry-run     With --once: only print what would happen, change nothing
    --minimized   Start the desktop app minimised
    -h, --help    Show this help

Config lives in the OS config dir (override with AUTOSORT_HOME).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("-h") || has("--help") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if let Some(bad) = args.iter().find(|a| !["--headless", "--once", "--dry-run", "--minimized"].contains(&a.as_str())) {
        eprintln!("autosort: unknown option '{bad}'\n\n{HELP}");
        return ExitCode::from(2);
    }
    if has("--dry-run") && !has("--once") {
        eprintln!("autosort: --dry-run only makes sense together with --once");
        return ExitCode::from(2);
    }

    let paths = Paths::discover();
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
    let engine = Engine::new(cfg.settle_seconds, Arc::new(History::persistent(paths.history())));
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
