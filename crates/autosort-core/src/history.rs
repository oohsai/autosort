use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::store;

const MAX_ENTRIES: usize = 50_000;

/// Remembers which files (path + modification time) were already handled, so that a rule like
/// "copy to backup" runs once per file instead of on every sweep.
#[derive(Debug, Default)]
pub struct History {
    seen: Mutex<HashSet<String>>,
    file: Option<PathBuf>,
}

fn key(path: &Path, modified: SystemTime) -> String {
    let secs = modified.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{}|{secs}", path.to_string_lossy())
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

    pub fn contains(&self, path: &Path, modified: SystemTime) -> bool {
        self.seen.lock().unwrap().contains(&key(path, modified))
    }

    pub fn insert(&self, path: &Path, modified: SystemTime) {
        let mut seen = self.seen.lock().unwrap();
        if seen.len() >= MAX_ENTRIES {
            seen.clear();
        }
        seen.insert(key(path, modified));
    }

    pub fn save(&self) {
        if let Some(file) = &self.file {
            let list: Vec<String> = self.seen.lock().unwrap().iter().cloned().collect();
            let _ = store::save_json(file, &list);
        }
    }
}
