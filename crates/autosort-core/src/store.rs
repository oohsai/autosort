use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Serialize, de::DeserializeOwned};

/// Where AutoSort keeps its files. Set `AUTOSORT_HOME` to override (portable installs, tests).
#[derive(Debug, Clone)]
pub struct Paths {
    pub dir: PathBuf,
}

impl Paths {
    pub fn discover() -> Paths {
        let dir = std::env::var_os("AUTOSORT_HOME").map(PathBuf::from).unwrap_or_else(|| {
            ProjectDirs::from("", "", "AutoSort")
                .map(|d| d.config_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".autosort"))
        });
        let _ = fs::create_dir_all(&dir);
        Paths { dir }
    }
    pub fn config(&self) -> PathBuf {
        self.dir.join("config.json")
    }
    pub fn activity(&self) -> PathBuf {
        self.dir.join("activity.json")
    }
    pub fn history(&self) -> PathBuf {
        self.dir.join("history.json")
    }
    pub fn stats(&self) -> PathBuf {
        self.dir.join("stats.json")
    }
    pub fn lock(&self) -> PathBuf {
        self.dir.join("autosort.lock")
    }
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Writes atomically (temp file + rename) so a crash never leaves a half-written config.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let mut f = File::create(&tmp)?;
    f.write_all(&data)?;
    f.sync_all()?;
    fs::rename(tmp, path)
}

/// Holds an exclusive lock for the life of the process so two AutoSorts never fight over the same folders.
pub struct InstanceLock(#[allow(dead_code)] File);

impl InstanceLock {
    pub fn acquire(paths: &Paths) -> Option<InstanceLock> {
        let f = File::options().create(true).truncate(false).write(true).open(paths.lock()).ok()?;
        f.try_lock().ok()?;
        Some(InstanceLock(f))
    }
}

/// Bounded, persisted activity log shared between the service thread, the UI and other AutoSort
/// processes (e.g. `autosort --sort` run from Finder). It merges with the file on disk so entries written by
/// another process are never lost.
pub struct ActivityLog {
    entries: std::sync::Mutex<Vec<crate::model::ActivityEntry>>,
    file: Option<PathBuf>,
    seen_mtime: std::sync::Mutex<Option<std::time::SystemTime>>,
}

const MAX_ACTIVITY: usize = 1000;

impl ActivityLog {
    pub fn new(file: Option<PathBuf>) -> Self {
        let entries = file.as_deref().and_then(load_json::<Vec<crate::model::ActivityEntry>>).unwrap_or_default();
        let mtime = file.as_deref().and_then(|f| fs::metadata(f).ok()).and_then(|m| m.modified().ok());
        ActivityLog { entries: std::sync::Mutex::new(entries), file, seen_mtime: std::sync::Mutex::new(mtime) }
    }

    fn disk_mtime(&self) -> Option<std::time::SystemTime> {
        self.file.as_deref().and_then(|f| fs::metadata(f).ok()).and_then(|m| m.modified().ok())
    }

    /// Adds entries another process wrote and copies over their "undone" marks.
    fn merge_from_disk(&self, entries: &mut Vec<crate::model::ActivityEntry>) {
        let Some(file) = &self.file else { return };
        let Some(disk) = load_json::<Vec<crate::model::ActivityEntry>>(file) else { return };
        let mut changed = false;
        for d in disk {
            match entries.iter_mut().find(|e| e.id == d.id) {
                Some(e) => {
                    if d.undone && !e.undone {
                        e.undone = true;
                    }
                }
                None => {
                    entries.push(d);
                    changed = true;
                }
            }
        }
        if changed {
            entries.sort_by_key(|e| e.time);
            let len = entries.len();
            if len > MAX_ACTIVITY {
                entries.drain(..len - MAX_ACTIVITY);
            }
        }
    }

    fn persist(&self, entries: &[crate::model::ActivityEntry]) {
        if let Some(f) = &self.file {
            let _ = save_json(f, &entries);
            *self.seen_mtime.lock().unwrap() = self.disk_mtime();
        }
    }

    pub fn push(&self, new: Vec<crate::model::ActivityEntry>) {
        let mut e = self.entries.lock().unwrap();
        self.merge_from_disk(&mut e);
        e.extend(new);
        let len = e.len();
        if len > MAX_ACTIVITY {
            e.drain(..len - MAX_ACTIVITY);
        }
        self.persist(&e);
    }

    pub fn snapshot(&self) -> Vec<crate::model::ActivityEntry> {
        let mut e = self.entries.lock().unwrap();
        if self.file.is_some() && *self.seen_mtime.lock().unwrap() != self.disk_mtime() {
            self.merge_from_disk(&mut e);
            *self.seen_mtime.lock().unwrap() = self.disk_mtime();
        }
        e.clone()
    }

    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        let mut e = self.entries.lock().unwrap();
        e.clear();
        self.persist(&e);
    }
}

impl ActivityLog {
    /// Reverses the action behind entry `id` and marks it undone. Returns a human-readable result.
    pub fn undo(&self, id: uuid::Uuid, history: &crate::history::History) -> Result<String, String> {
        let op = {
            let e = self.entries.lock().unwrap();
            let entry = e.iter().find(|x| x.id == id).ok_or("that entry no longer exists")?;
            if entry.undone {
                return Err("already undone".into());
            }
            entry.undo.clone().ok_or("this step cannot be undone")?
        };
        let message = crate::undo::apply(&op, history)?;
        let mut e = self.entries.lock().unwrap();
        if let Some(entry) = e.iter_mut().find(|x| x.id == id) {
            entry.undone = true;
        }
        self.persist(&e);
        Ok(message)
    }
}
