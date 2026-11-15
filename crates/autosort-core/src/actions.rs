use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

use crate::fileinfo::FileInfo;
use crate::model::{Action, ActionKind, ConflictPolicy, UndoOp};
use crate::template::{expand, expand_tilde};

/// What an action did (or, in a dry run, would do).
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Where the file lives afterwards; later actions in the rule operate on this path.
    pub new_path: PathBuf,
    pub message: String,
    /// True when the file is gone from its watched folder (trashed), so processing must stop.
    pub removed: bool,
    /// How to reverse this step, when it changed something on disk.
    pub undo: Option<UndoOp>,
}

const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

pub fn execute(
    action: &Action,
    info: &FileInfo,
    folder: &Path,
    dry_run: bool,
    now: DateTime<Local>,
) -> Result<Outcome, String> {
    let param = expand(action.parameter.trim(), info, folder, now);
    if action.kind.uses_parameter() && param.is_empty() {
        return Err(format!("“{}” needs a value", action.kind.label()));
    }
    match action.kind {
        ActionKind::Move => transfer(info, folder, &param, action.conflict, dry_run, true),
        ActionKind::Copy => transfer(info, folder, &param, action.conflict, dry_run, false),
        ActionKind::Rename => rename(info, &param, action.conflict, dry_run),
        ActionKind::Trash => {
            if !dry_run {
                trash::delete(&info.path).map_err(|e| format!("could not move to trash: {e}"))?;
            }
            Ok(Outcome { new_path: info.path.clone(), message: "Moved to Trash".into(), removed: true, undo: None })
        }
        ActionKind::RunCommand => run_command(info, &action.parameter, dry_run),
    }
}

fn transfer(
    info: &FileInfo,
    folder: &Path,
    dest_dir_str: &str,
    policy: ConflictPolicy,
    dry_run: bool,
    is_move: bool,
) -> Result<Outcome, String> {
    let mut dest_dir = expand_tilde(dest_dir_str);
    if dest_dir.is_relative() {
        dest_dir = folder.join(dest_dir); // relative destinations are relative to the watched folder
    }
    let verb = if is_move { "Moved" } else { "Copied" };
    let src_dir = info.path.parent().unwrap_or(Path::new(""));
    if is_move && same_dir(&dest_dir, src_dir) {
        return Ok(Outcome { new_path: info.path.clone(), message: "Already in place".into(), removed: false, undo: None });
    }
    let dest = dest_dir.join(&info.name);
    let Some((final_path, overwrite)) = resolve_conflict(dest, policy) else {
        return Ok(Outcome {
            new_path: info.path.clone(),
            message: format!("Skipped: {} already exists in {}", info.name, dest_dir.display()),
            removed: false,
            undo: None,
        });
    };
    if !dry_run {
        fs::create_dir_all(&dest_dir).map_err(|e| format!("cannot create {}: {e}", dest_dir.display()))?;
        if overwrite && final_path.exists() {
            fs::remove_file(&final_path).map_err(|e| format!("cannot replace {}: {e}", final_path.display()))?;
        }
        if is_move {
            move_file(&info.path, &final_path).map_err(|e| format!("move failed: {e}"))?;
        } else {
            fs::copy(&info.path, &final_path).map_err(|e| format!("copy failed: {e}"))?;
        }
    }
    Ok(Outcome {
        new_path: if is_move { final_path.clone() } else { info.path.clone() },
        message: format!("{verb} to {}", final_path.display()),
        removed: false,
        undo: (!dry_run).then(|| {
            if is_move {
                UndoOp::Moved { from: info.path.clone(), to: final_path.clone() }
            } else {
                UndoOp::Copied { copy: final_path.clone() }
            }
        }),
    })
}

fn rename(info: &FileInfo, pattern: &str, policy: ConflictPolicy, dry_run: bool) -> Result<Outcome, String> {
    let base: String = pattern
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    let base = base.trim().to_string();
    if base.is_empty() {
        return Err("rename pattern produced an empty name".into());
    }
    let ext = info.path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let dir = info.path.parent().unwrap_or(Path::new(""));
    let dest = dir.join(format!("{base}{ext}"));
    if dest == info.path {
        return Ok(Outcome { new_path: info.path.clone(), message: "Name unchanged".into(), removed: false, undo: None });
    }
    let Some((final_path, overwrite)) = resolve_conflict(dest, policy) else {
        return Ok(Outcome { new_path: info.path.clone(), message: "Skipped: a file with that name exists".into(), removed: false, undo: None });
    };
    if !dry_run {
        if overwrite && final_path.exists() {
            fs::remove_file(&final_path).map_err(|e| format!("cannot replace file: {e}"))?;
        }
        fs::rename(&info.path, &final_path).map_err(|e| format!("rename failed: {e}"))?;
    }
    let new_name = final_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let undo = (!dry_run).then(|| UndoOp::Moved { from: info.path.clone(), to: final_path.clone() });
    Ok(Outcome { new_path: final_path, message: format!("Renamed to {new_name}"), removed: false, undo })
}

fn run_command(info: &FileInfo, script: &str, dry_run: bool) -> Result<Outcome, String> {
    let script = script.trim();
    if dry_run {
        return Ok(Outcome { new_path: info.path.clone(), message: format!("Would run: {script}"), removed: false, undo: None });
    }
    #[cfg(unix)]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script).arg("autosort").arg(&info.path); // the file is "$1" and $AUTOSORT_FILE
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.arg("/C").raw_arg(script); // the file is %AUTOSORT_FILE%
        c
    };
    let mut child = cmd
        .env("AUTOSORT_FILE", &info.path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start command: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return Ok(Outcome { new_path: info.path.clone(), message: "Command finished".into(), removed: false, undo: None });
            }
            Ok(Some(status)) => return Err(format!("command exited with {status}")),
            Ok(None) if started.elapsed() > COMMAND_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("command timed out after 60s".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("command failed: {e}")),
        }
    }
}

// ------------------------------------------------------------------- helpers

fn same_dir(a: &Path, b: &Path) -> bool {
    let ca = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

/// Returns the path to write to and whether an existing file must be replaced, or `None` to skip.
fn resolve_conflict(dest: PathBuf, policy: ConflictPolicy) -> Option<(PathBuf, bool)> {
    if !dest.exists() {
        return Some((dest, false));
    }
    match policy {
        ConflictPolicy::Skip => None,
        ConflictPolicy::Overwrite => Some((dest, true)),
        ConflictPolicy::Rename => {
            let dir = dest.parent().map(Path::to_path_buf).unwrap_or_default();
            let stem = dest.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = dest.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            (2u32..).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).map(|p| (p, false))
        }
    }
}

/// `rename`, falling back to copy + delete when crossing filesystems.
pub(crate) fn move_file(src: &Path, dest: &Path) -> io::Result<()> {
    if fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    fs::copy(src, dest)?;
    fs::remove_file(src)
}
