use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Duration, Local};

use crate::actions;
use crate::fileinfo::FileInfo;
use crate::history::History;
use crate::model::{ActivityEntry, UndoOp, WatchedFolder};
use uuid::Uuid;

/// Extensions of files that are still being written by browsers and download managers.
const TEMP_EXTENSIONS: &[&str] =
    &["crdownload", "download", "part", "partial", "tmp", "opdownload", "filepart", "aria2", "!ut"];

pub struct Engine {
    pub settle: Duration,
    history: Arc<History>,
}

impl Engine {
    pub fn new(settle_seconds: u64, history: Arc<History>) -> Self {
        Engine { settle: Duration::seconds(settle_seconds as i64), history }
    }

    /// Files in `root` that rules may act on (no hidden files, no temp/partial downloads).
    pub fn candidate_files(root: &Path, recursive: bool) -> Vec<PathBuf> {
        let mut out = Vec::new();
        walk(root, recursive, &mut out);
        out.sort();
        out
    }

    /// Runs every enabled rule of `folder` against its files. With `dry_run` nothing on disk changes
    /// and nothing is remembered; the returned entries describe what *would* happen.
    pub fn process(&self, folder: &WatchedFolder, dry_run: bool, now: DateTime<Local>) -> Vec<ActivityEntry> {
        let root = folder.root();
        let rules: Vec<_> =
            folder.rules.iter().filter(|r| r.enabled && !r.conditions.is_empty() && !r.actions.is_empty()).collect();
        let mut entries = Vec::new();
        if rules.is_empty() || !root.is_dir() {
            return entries;
        }
        for path in Self::candidate_files(&root, folder.include_subfolders) {
            let Ok(mut info) = FileInfo::load(&path) else { continue };
            let age = now - info.modified;
            if age >= Duration::zero() && age < self.settle {
                continue; // too fresh, probably still being written
            }
            let modified: SystemTime = info.modified.into();
            if self.history.contains(&path, modified) {
                continue;
            }
            let mut matched_any = false;
            'rules: for rule in &rules {
                if !rule.matches(&info, now) {
                    continue;
                }
                matched_any = true;
                for action in &rule.actions {
                    let original_name = info.name.clone();
                    let entry = |detail: String, success: bool, undo: Option<UndoOp>| ActivityEntry {
                        id: Uuid::new_v4(),
                        undo,
                        undone: false,
                        time: now,
                        folder: folder.path.clone(),
                        rule: rule.name.clone(),
                        file: original_name.clone(),
                        action: action.kind.label().to_string(),
                        detail,
                        success,
                        dry_run,
                    };
                    match actions::execute(action, &info, &root, dry_run, now) {
                        Ok(outcome) => {
                            entries.push(entry(outcome.message.clone(), true, outcome.undo.clone()));
                            info = info.relocated(outcome.new_path);
                            if outcome.removed {
                                break 'rules;
                            }
                        }
                        Err(err) => {
                            entries.push(entry(err, false, None));
                            break 'rules; // never cascade after a failure
                        }
                    }
                }
                if rule.stop_processing {
                    break;
                }
            }
            if matched_any && !dry_run {
                // Fingerprint the file where it ended up (and with its current mtime) so it is not
                // processed again — this is what prevents copy/rename rules from looping forever.
                if let Ok(now_info) = FileInfo::load(&info.path) {
                    self.history.insert(&info.path, now_info.modified.into());
                } else {
                    self.history.insert(&info.path, modified);
                }
            }
        }
        if !dry_run && !entries.is_empty() {
            self.history.save();
        }
        entries
    }
}

fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with("~$")
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_name(&name) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_dir() {
            if recursive {
                walk(&path, true, out);
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
