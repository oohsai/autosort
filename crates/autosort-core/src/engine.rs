use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Duration, Local};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use uuid::Uuid;

use crate::actions;
use crate::dupes::DupIndex;
use crate::fileinfo::FileInfo;
use crate::history::History;
use crate::model::{ActivityEntry, Rule, Schedule, UndoOp, WatchedFolder};
use crate::stats::Stats;

/// Extensions of files that are still being written by browsers and download managers.
const TEMP_EXTENSIONS: &[&str] =
    &["crdownload", "download", "part", "partial", "tmp", "opdownload", "filepart", "aria2", "!ut"];

/// How a run behaves.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Plan only: nothing on disk changes and nothing is remembered. Also includes rules that are
    /// awaiting approval or not yet due, so a preview shows the full picture.
    pub dry_run: bool,
    /// Run scheduled rules now even if they are not due ("Run now").
    pub force_scheduled: bool,
    /// Consider files even if they were handled before (an explicit request from the user).
    pub ignore_history: bool,
    /// Don't wait for files to settle (an explicit request from the user).
    pub ignore_settle: bool,
    /// Only these files (instead of scanning the whole folder).
    pub only: Option<Vec<PathBuf>>,
}

pub struct Engine {
    pub settle: Duration,
    history: Arc<History>,
    stats: Arc<Stats>,
}

/// Compiled ignore patterns: names match at any depth, patterns containing `/` match the path inside
/// the watched folder.
struct Ignore {
    names: GlobSet,
    paths: GlobSet,
}

impl Ignore {
    fn new(patterns: &[String]) -> Self {
        let (mut names, mut paths) = (GlobSetBuilder::new(), GlobSetBuilder::new());
        let build = |p: &str| GlobBuilder::new(p).case_insensitive(true).literal_separator(false).build().ok();
        for raw in patterns.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            let p = raw.trim_end_matches('/');
            if p.contains('/') {
                let p = p.trim_start_matches("./").trim_start_matches('/');
                if let Some(g) = build(p) {
                    paths.add(g);
                }
                if let Some(g) = build(&format!("{p}/**")) {
                    paths.add(g); // ignoring a folder ignores everything inside it
                }
            } else if let Some(g) = build(p) {
                names.add(g);
            }
        }
        let empty = || GlobSetBuilder::new().build().unwrap();
        Ignore { names: names.build().unwrap_or_else(|_| empty()), paths: paths.build().unwrap_or_else(|_| empty()) }
    }

    fn matches(&self, name: &str, rel: &Path) -> bool {
        self.names.is_match(name) || self.paths.is_match(rel.to_string_lossy().replace('\\', "/"))
    }
}

impl Engine {
    pub fn new(settle_seconds: u64, history: Arc<History>) -> Self {
        Engine { settle: Duration::seconds(settle_seconds as i64), history, stats: Arc::new(Stats::in_memory()) }
    }

    /// Records per-rule counters and scheduled-run times here.
    pub fn with_stats(mut self, stats: Arc<Stats>) -> Self {
        self.stats = stats;
        self
    }

    /// Files in `root` that rules may act on (no hidden files, no temp/partial downloads).
    pub fn candidate_files(root: &Path, recursive: bool) -> Vec<PathBuf> {
        Self::candidate_files_ignoring(root, recursive, &[])
    }

    pub fn candidate_files_ignoring(root: &Path, recursive: bool, ignore: &[String]) -> Vec<PathBuf> {
        let mut out = Vec::new();
        walk(root, root, recursive, &Ignore::new(ignore), &mut out);
        out.sort();
        out
    }

    /// Runs every enabled rule of `folder` against its files. With `dry_run` nothing on disk changes
    /// and nothing is remembered; the returned entries describe what *would* happen.
    pub fn process(&self, folder: &WatchedFolder, dry_run: bool, now: DateTime<Local>) -> Vec<ActivityEntry> {
        self.process_with(folder, &RunOptions { dry_run, force_scheduled: dry_run, ..Default::default() }, now)
    }

    pub fn process_with(&self, folder: &WatchedFolder, opts: &RunOptions, now: DateTime<Local>) -> Vec<ActivityEntry> {
        let root = folder.root();
        if !root.is_dir() {
            return Vec::new();
        }
        let files = match &opts.only {
            Some(files) => files.clone(),
            None => Self::candidate_files_ignoring(&root, folder.include_subfolders, &folder.ignore),
        };
        self.run(folder, &root, files, opts, now)
    }

    fn run(&self, folder: &WatchedFolder, root: &Path, files: Vec<PathBuf>, opts: &RunOptions, now: DateTime<Local>) -> Vec<ActivityEntry> {
        let dry_run = opts.dry_run;
        let rules = self.active_rules(folder, opts, now);
        let mut entries = Vec::new();
        if rules.is_empty() {
            return entries;
        }
        let ignore = Ignore::new(&folder.ignore);
        let dup_index = Arc::new(DupIndex::new(root));
        let mut ran_scheduled: HashSet<Uuid> = HashSet::new();

        for path in files {
            if opts.only.is_some() {
                // explicit file list: still respect ignore patterns and temp files
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if !path.is_file() || ignore.matches(&name, rel) {
                    continue;
                }
            }
            let Ok(info) = FileInfo::load(&path) else { continue };
            let mut info = info.with_dup_index(dup_index.clone());
            let age = now - info.modified;
            if !opts.ignore_settle && age >= Duration::zero() && age < self.settle {
                continue; // too fresh, probably still being written
            }
            let modified: SystemTime = info.modified.into();
            if !opts.ignore_history && self.history.contains(None, &path, modified) {
                continue;
            }
            let mut acted: Vec<(Uuid, u64)> = Vec::new();

            'rules: for (rule, pending) in &rules {
                if !opts.ignore_history && self.history.contains(Some(rule.id), &info.path, info.modified.into()) {
                    continue;
                }
                if !rule.matches(&info, now) {
                    continue;
                }
                let rule_label =
                    if *pending { format!("{} (awaiting approval)", rule.name) } else { rule.name.clone() };
                let mut ok_actions = 0u64;
                let mut stop_everything = false;
                for action in &rule.actions {
                    let original_name = info.name.clone();
                    let entry = |detail: String, success: bool, undo: Option<UndoOp>| ActivityEntry {
                        id: Uuid::new_v4(),
                        undo,
                        undone: false,
                        time: now,
                        folder: folder.path.clone(),
                        rule: rule_label.clone(),
                        file: original_name.clone(),
                        action: action.kind.label().to_string(),
                        detail,
                        success,
                        dry_run,
                    };
                    match actions::execute(action, &info, root, dry_run, now) {
                        Ok(outcome) => {
                            ok_actions += 1;
                            entries.push(entry(outcome.message.clone(), true, outcome.undo.clone()));
                            info = info.relocated(outcome.new_path);
                            if outcome.removed {
                                stop_everything = true;
                                break;
                            }
                        }
                        Err(err) => {
                            entries.push(entry(err, false, None));
                            stop_everything = true; // never cascade after a failure
                            break;
                        }
                    }
                }
                if !dry_run {
                    acted.push((rule.id, ok_actions));
                    if rule.schedule != Schedule::Always {
                        ran_scheduled.insert(rule.id);
                    }
                }
                if stop_everything || rule.stop_processing {
                    break 'rules;
                }
            }

            if !dry_run && !acted.is_empty() {
                // Remember every rule that acted, at the file's *final* location and mtime, so none of them
                // fires on it again — this is what stops copy/rename rules from looping forever. Other
                // rules (e.g. a scheduled cleanup) remain free to look at the file later.
                let (final_path, final_mtime) = match FileInfo::load(&info.path) {
                    Ok(f) => (f.path.clone(), SystemTime::from(f.modified)),
                    Err(_) => (info.path.clone(), modified),
                };
                for (rule_id, ok_actions) in acted {
                    self.history.insert(Some(rule_id), &final_path, final_mtime);
                    if ok_actions > 0 {
                        self.stats.record(rule_id, ok_actions, now);
                    }
                }
            }
        }

        if !dry_run {
            for id in ran_scheduled {
                self.stats.set_last_scheduled_run(id, now);
            }
            // forced runs of scheduled rules that matched nothing still count as having run
            if opts.force_scheduled {
                for (rule, _) in &rules {
                    if rule.schedule != Schedule::Always {
                        self.stats.set_last_scheduled_run(rule.id, now);
                    }
                }
            }
            if !entries.is_empty() {
                self.history.save();
            }
            self.stats.save();
        }
        entries
    }

    /// Rules that may act in this run, each paired with whether it is "awaiting approval" (preview only).
    fn active_rules<'a>(&self, folder: &'a WatchedFolder, opts: &RunOptions, now: DateTime<Local>) -> Vec<(&'a Rule, bool)> {
        let mut out = Vec::new();
        for r in folder.rules.iter().filter(|r| r.enabled && !r.conditions.is_empty() && !r.actions.is_empty()) {
            if !r.approved && !opts.dry_run {
                continue; // learning mode: only previewed, never executed
            }
            if r.schedule != Schedule::Always && !opts.force_scheduled && !self.schedule_due(r, opts.dry_run, now) {
                continue;
            }
            out.push((r, !r.approved));
        }
        out
    }

    /// A scheduled rule is due once its latest scheduled moment is newer than its last run. A brand-new
    /// rule starts counting from now, so creating "daily at 09:00" at 15:00 does not fire immediately.
    fn schedule_due(&self, rule: &Rule, dry_run: bool, now: DateTime<Local>) -> bool {
        let Some(occurrence) = rule.schedule.latest_occurrence(now) else { return true };
        match self.stats.last_scheduled_run(rule.id) {
            Some(last) => last < occurrence,
            None => {
                if !dry_run {
                    self.stats.set_last_scheduled_run(rule.id, now);
                    self.stats.save();
                }
                false
            }
        }
    }
}

fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with("~$")
}

fn walk(root: &Path, dir: &Path, recursive: bool, ignore: &Ignore, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_name(&name) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        if ignore.matches(&name, &rel) {
            continue;
        }
        if ft.is_dir() {
            if recursive {
                walk(root, &path, true, ignore, out);
            }
        } else if ft.is_file() {
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !TEMP_EXTENSIONS.contains(&ext.as_str()) {
                out.push(path);
            }
        }
        // symlinks are deliberately ignored
    }
}

