//! Sharing rules as files.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::Rule;

const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct RuleFile {
    autosort_rules: u32,
    rules: Vec<Rule>,
}

/// Serialises rules to a portable JSON document.
pub fn export_rules(rules: &[Rule]) -> String {
    serde_json::to_string_pretty(&RuleFile { autosort_rules: FORMAT_VERSION, rules: rules.to_vec() })
        .unwrap_or_else(|_| "{}".to_string())
}

/// Parses an exported file. Every rule, condition and action gets a fresh id so imports never collide
/// with existing rules; when `needs_approval` (learning mode) imported rules start unapproved.
pub fn import_rules(json: &str, needs_approval: bool) -> Result<Vec<Rule>, String> {
    let file: RuleFile = serde_json::from_str(json).map_err(|_| "this is not an AutoSort rules file".to_string())?;
    if file.autosort_rules > FORMAT_VERSION {
        return Err("this rules file was made by a newer version of AutoSort".into());
    }
    let mut rules = file.rules;
    for r in &mut rules {
        r.id = Uuid::new_v4();
        for c in &mut r.conditions {
            c.id = Uuid::new_v4();
        }
        for a in &mut r.actions {
            a.id = Uuid::new_v4();
        }
        if needs_approval {
            r.approved = false;
        }
    }
    Ok(rules)
}
