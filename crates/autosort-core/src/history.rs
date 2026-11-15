use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use uuid::Uuid;

use crate::store;

const MAX_ENTRIES: usize = 50_000;

/// Remembers which (rule, file, modification time) combinations were already handled, so that a rule like
/// "copy to backup" runs once per file instead of on every sweep — while *other* rules (say, a scheduled
/// "archive after 30 days" rule) can still look at the same file later.
///
/// An entry without a rule means "leave this file alone for every rule" (used when you undo a move).
#[derive(Debug, Default)]
pub struct History {
    seen: Mutex<HashSet<String>>,
    file: Option<PathBuf>,
}

fn key(rule: Option<Uuid>, path: &Path, modified: SystemTime) -> String {
    let secs = modified.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let base = format!("{}|{secs}", path.to_string_lossy());
    match rule {
        Some(id) => format!("{id}|{base}"),
        None => base,
    }
}

impl History {
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Loads (or starts) a history persisted at `file`.
    pub fn persistent(file: PathBuf) -> Self {
        let seen: HashSet<String> = store::load_json::<Vec<String>>(&file).unwrap_or_default().into_iter().collect();
        History { seen: Mutex::new(seen), file: Some(file) }
    }

    pub fn contains(&self, rule: Option<Uuid>, path: &Path, modified: SystemTime) -> bool {
        self.seen.lock().unwrap().contains(&key(rule, path, modified))
    }

    pub fn insert(&self, rule: Option<Uuid>, path: &Path, modified: SystemTime) {
        let mut seen = self.seen.lock().unwrap();
        if seen.len() >= MAX_ENTRIES {
            seen.clear();
        }
        seen.insert(key(rule, path, modified));
    }

    /// Pulls in entries another AutoSort process (e.g. `autosort --sort` from Finder) wrote to disk.
    pub fn refresh(&self) {
        if let Some(file) = &self.file {
            if let Some(on_disk) = store::load_json::<Vec<String>>(file) {
                self.seen.lock().unwrap().extend(on_disk);
            }
        }
    }

    /// Saves the union of memory and disk, so two processes never erase each other's records.
    pub fn save(&self) {
        if self.file.is_some() {
            self.refresh();
        }
        if let Some(file) = &self.file {
            let list: Vec<String> = self.seen.lock().unwrap().iter().cloned().collect();
            let _ = store::save_json(file, &list);
        }
    }
}
