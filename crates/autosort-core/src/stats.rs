use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::store;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RuleStats {
    /// Files this rule has acted on.
    #[serde(default)]
    pub files: u64,
    /// Individual actions that succeeded.
    #[serde(default)]
    pub actions: u64,
    #[serde(default)]
    pub last_match: Option<DateTime<Local>>,
    /// When a scheduled rule last ran (used to decide when it is next due).
    #[serde(default)]
    pub last_scheduled_run: Option<DateTime<Local>>,
}

/// Per-rule counters, persisted separately from the config so the background service can update them
/// without fighting the UI over `config.json`.
#[derive(Debug, Default)]
pub struct Stats {
    map: Mutex<HashMap<Uuid, RuleStats>>,
    file: Option<PathBuf>,
}

impl Stats {
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn persistent(file: PathBuf) -> Self {
        let map = store::load_json::<HashMap<Uuid, RuleStats>>(&file).unwrap_or_default();
        Stats { map: Mutex::new(map), file: Some(file) }
    }

    pub fn get(&self, rule: Uuid) -> RuleStats {
        self.map.lock().unwrap().get(&rule).cloned().unwrap_or_default()
    }

    /// Counts one file handled by `rule`, with `actions` successful actions.
    pub fn record(&self, rule: Uuid, actions: u64, when: DateTime<Local>) {
        let mut m = self.map.lock().unwrap();
        let s = m.entry(rule).or_default();
        s.files += 1;
        s.actions += actions;
        s.last_match = Some(when);
    }

    pub fn last_scheduled_run(&self, rule: Uuid) -> Option<DateTime<Local>> {
        self.map.lock().unwrap().get(&rule).and_then(|s| s.last_scheduled_run)
    }

    pub fn set_last_scheduled_run(&self, rule: Uuid, when: DateTime<Local>) {
        self.map.lock().unwrap().entry(rule).or_default().last_scheduled_run = Some(when);
    }

    pub fn forget(&self, rule: Uuid) {
        self.map.lock().unwrap().remove(&rule);
    }

    /// Merges counters written by another AutoSort process (larger counts and later times win).
    pub fn refresh(&self) {
        let Some(file) = &self.file else { return };
        let Some(disk) = store::load_json::<HashMap<Uuid, RuleStats>>(file) else { return };
        let mut m = self.map.lock().unwrap();
        for (id, d) in disk {
            let s = m.entry(id).or_default();
            s.files = s.files.max(d.files);
            s.actions = s.actions.max(d.actions);
            s.last_match = s.last_match.max(d.last_match);
            s.last_scheduled_run = s.last_scheduled_run.max(d.last_scheduled_run);
        }
    }

    pub fn save(&self) {
        if self.file.is_some() {
            self.refresh();
        }
        if let Some(file) = &self.file {
            let _ = store::save_json(file, &*self.map.lock().unwrap());
        }
    }
}
