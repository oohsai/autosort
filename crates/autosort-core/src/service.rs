use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Local;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use uuid::Uuid;

use crate::engine::{Engine, RunOptions};
use crate::history::History;
use crate::stats::Stats;
use crate::model::{ActivityEntry, AppConfig, WatchedFolder};

/// Quiet period after the last filesystem event before a folder is processed.
const DEBOUNCE: Duration = Duration::from_millis(1500);
/// Periodic full sweep: catches files that finished settling since the last event.
const SWEEP: Duration = Duration::from_secs(30);

enum Cmd {
    Fs(Vec<std::path::PathBuf>),
    Reload,
    RunNow,
    /// A normal (non-forced) pass over every enabled folder, e.g. right after approving a rule.
    Sweep,
    Pause(bool),
    Stop,
}

pub type EntrySink = Arc<dyn Fn(Vec<ActivityEntry>) + Send + Sync>;

/// Background worker: watches the configured folders and runs the engine when they change.
pub struct Service {
    tx: Sender<Cmd>,
    join: Option<JoinHandle<()>>,
}

impl Service {
    pub fn start(config: Arc<Mutex<AppConfig>>, history: Arc<History>, stats: Arc<Stats>, sink: EntrySink) -> Service {
        let (tx, rx) = mpsc::channel();
        let worker_tx = tx.clone();
        let join = std::thread::Builder::new()
            .name("autosort-service".into())
            .spawn(move || worker(rx, worker_tx, config, history, stats, sink))
            .expect("spawn service thread");
        Service { tx, join: Some(join) }
    }

    /// Call after the shared config changed (folders added/removed/enabled).
    pub fn reload(&self) {
        let _ = self.tx.send(Cmd::Reload);
    }
    pub fn run_now(&self) {
        let _ = self.tx.send(Cmd::RunNow);
    }
    /// Re-checks all folders immediately without forcing scheduled rules to run.
    pub fn sweep(&self) {
        let _ = self.tx.send(Cmd::Sweep);
    }
    pub fn set_paused(&self, paused: bool) {
        let _ = self.tx.send(Cmd::Pause(paused));
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn build_watcher(tx: &Sender<Cmd>, folders: &[WatchedFolder]) -> Option<RecommendedWatcher> {
    let sender = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if !ev.kind.is_access() {
                let _ = sender.send(Cmd::Fs(ev.paths));
            }
        }
    })
    .map_err(|e| eprintln!("autosort: cannot create file watcher: {e}"))
    .ok()?;
    for f in folders.iter().filter(|f| f.enabled) {
        let root = f.root();
        if root.is_dir() {
            let mode = if f.include_subfolders { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
            if let Err(e) = watcher.watch(&root, mode) {
                eprintln!("autosort: cannot watch {}: {e}", root.display());
            }
        }
    }
    Some(watcher)
}

// `watcher` is held only to keep the OS watch alive (RAII); reassigning it drops the old one.
#[allow(unused_assignments, unused_variables)]
fn worker(
    rx: mpsc::Receiver<Cmd>,
    tx: Sender<Cmd>,
    config: Arc<Mutex<AppConfig>>,
    history: Arc<History>,
    stats: Arc<Stats>,
    sink: EntrySink,
) {
    let snapshot = || config.lock().unwrap().clone();
    let run_folders = |folders: Vec<&WatchedFolder>, settle: u64, force: bool| {
        history.refresh();
        stats.refresh();
        let engine = Engine::new(settle, history.clone()).with_stats(stats.clone());
        let opts = RunOptions { force_scheduled: force, ..Default::default() };
        for f in folders {
            let entries = engine.process_with(f, &opts, Local::now());
            if !entries.is_empty() {
                sink(entries);
            }
        }
    };

    let mut paused = false;
    let mut watcher = build_watcher(&tx, &snapshot().folders);
    let mut pending: HashMap<Uuid, Instant> = HashMap::new();
    let mut last_sweep = Instant::now();
    {
        let cfg = snapshot();
        run_folders(cfg.folders.iter().filter(|f| f.enabled).collect(), cfg.settle_seconds, false);
    }

    loop {
        let now = Instant::now();
        let next_due = pending.values().min().copied().unwrap_or(now + SWEEP);
        let sweep_due = last_sweep + SWEEP;
        let wait = next_due.min(sweep_due).saturating_duration_since(now).max(Duration::from_millis(20));

        match rx.recv_timeout(wait) {
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Cmd::Reload) => {
                pending.clear();
                watcher = if paused { None } else { build_watcher(&tx, &snapshot().folders) };
            }
            Ok(Cmd::RunNow) => {
                if !paused {
                    let cfg = snapshot();
                    run_folders(cfg.folders.iter().filter(|f| f.enabled).collect(), cfg.settle_seconds, true);
                }
            }
            Ok(Cmd::Sweep) => {
                if !paused {
                    let cfg = snapshot();
                    run_folders(cfg.folders.iter().filter(|f| f.enabled).collect(), cfg.settle_seconds, false);
                }
            }
            Ok(Cmd::Pause(p)) => {
                paused = p;
                pending.clear();
                if p {
                    watcher = None;
                } else {
                    watcher = build_watcher(&tx, &snapshot().folders);
                    let cfg = snapshot();
                    run_folders(cfg.folders.iter().filter(|f| f.enabled).collect(), cfg.settle_seconds, false);
                }
            }
            Ok(Cmd::Fs(paths)) => {
                if !paused {
                    let cfg = snapshot();
                    for f in cfg.folders.iter().filter(|f| f.enabled) {
                        let root = std::fs::canonicalize(f.root()).unwrap_or_else(|_| f.root());
                        let hit = paths.iter().any(|p| p.starts_with(&root) || p.starts_with(f.root()));
                        if hit {
                            pending.insert(f.id, Instant::now() + DEBOUNCE);
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        if paused {
            continue;
        }
        let now = Instant::now();
        let due: Vec<Uuid> = pending.iter().filter(|(_, t)| **t <= now).map(|(id, _)| *id).collect();
        let sweep = now.duration_since(last_sweep) >= SWEEP;
        if !due.is_empty() || sweep {
            let cfg = snapshot();
            let folders: Vec<&WatchedFolder> =
                cfg.folders.iter().filter(|f| f.enabled && (sweep || due.contains(&f.id))).collect();
            for id in &due {
                pending.remove(id);
            }
            if sweep {
                last_sweep = now;
                pending.clear();
            }
            run_folders(folders, cfg.settle_seconds, false);
        }
    }
}
