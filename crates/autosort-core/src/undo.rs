use std::fs;

use crate::actions::move_file;
use crate::fileinfo::FileInfo;
use crate::history::History;
use crate::model::UndoOp;

/// Reverses one recorded action. Never overwrites anything: if the situation changed since the action
/// ran (file gone, original spot taken) it refuses with an explanation.
pub fn apply(op: &UndoOp, history: &History) -> Result<String, String> {
    match op {
        UndoOp::Moved { from, to } => {
            if !to.exists() {
                return Err(format!(
                    "{} is no longer there (if later steps changed it, undo those first)",
                    to.display()
                ));
            }
            if from.exists() {
                return Err(format!("something already exists at {}", from.display()));
            }
            if let Some(dir) = from.parent() {
                fs::create_dir_all(dir).map_err(|e| format!("cannot recreate {}: {e}", dir.display()))?;
            }
            move_file(to, from).map_err(|e| format!("could not move it back: {e}"))?;
            // The restored file must not be sorted again straight away.
            if let Ok(info) = FileInfo::load(from) {
                history.insert(None, from, info.modified.into());
                history.save();
            }
            Ok(format!("Restored to {}", from.display()))
        }
        UndoOp::Copied { copy } => {
            if !copy.exists() {
                return Err(format!("{} is already gone", copy.display()));
            }
            trash::delete(copy).map_err(|e| format!("could not trash the copy: {e}"))?;
            Ok(format!("Moved the copy {} to the Trash", copy.display()))
        }
    }
}
