//! Runs the real `autosort` binary.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_autosort"))
        .env("AUTOSORT_HOME", home)
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run autosort")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// A config watching `watched` with a pdf -> Sorted rule (approved or not).
fn setup(approved: bool) -> (TempDir, TempDir) {
    let home = TempDir::new().unwrap();
    let watched = TempDir::new().unwrap();
    let cfg = serde_json::json!({
        "notifications": false,
        "folders": [{
            "path": watched.path(),
            "rules": [{
                "name": "pdfs",
                "approved": approved,
                "conditions": [{"attribute": "extension", "op": "equals", "value": "pdf"}],
                "actions": [{"kind": "move", "parameter": "{folder}/Sorted/{kind}"}]
            }]
        }]
    });
    fs::write(home.path().join("config.json"), cfg.to_string()).unwrap();
    (home, watched)
}

#[test]
fn help_lists_the_new_commands() {
    let home = TempDir::new().unwrap();
    let o = run(home.path(), &["--help"]);
    assert!(o.status.success());
    let t = text(&o);
    for s in ["--sort", "--install-integration", "--uninstall-integration", "--headless"] {
        assert!(t.contains(s), "help should mention {s}");
    }
}

#[test]
fn sort_files_a_brand_new_file_immediately_and_logs_it() {
    let (home, watched) = setup(true);
    let f = watched.path().join("fresh.pdf"); // brand new: the background sweep would wait for it to settle
    fs::write(&f, "x").unwrap();
    let o = run(home.path(), &["--sort", f.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(watched.path().join("Sorted/PDFs/fresh.pdf").exists());
    assert!(text(&o).contains("fresh.pdf"));
    let log = fs::read_to_string(home.path().join("activity.json")).unwrap();
    assert!(log.contains("fresh.pdf"), "the move shows up in Activity (with its undo)");
}

#[test]
fn sort_expands_folders_and_handles_awkward_names() {
    let (home, watched) = setup(true);
    fs::write(watched.path().join("my report (final) 'v2'.pdf"), "x").unwrap();
    fs::write(watched.path().join("note.txt"), "x").unwrap();
    let o = run(home.path(), &["--sort", watched.path().to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(watched.path().join("Sorted/PDFs/my report (final) 'v2'.pdf").exists());
    assert!(watched.path().join("note.txt").exists(), "no rule matches a txt file");
}

#[test]
fn sort_rejects_files_outside_watched_folders() {
    let (home, _watched) = setup(true);
    let elsewhere = TempDir::new().unwrap();
    let f = elsewhere.path().join("a.pdf");
    fs::write(&f, "x").unwrap();
    let o = run(home.path(), &["--sort", f.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o).contains("not inside a watched folder"), "{}", text(&o));
    assert!(f.exists(), "nothing was touched");
}

#[test]
fn sort_needs_a_path() {
    let (home, _w) = setup(true);
    assert_eq!(run(home.path(), &["--sort"]).status.code(), Some(2));
}

#[test]
fn sort_never_runs_rules_that_await_approval_and_says_so() {
    let (home, watched) = setup(false);
    let f = watched.path().join("a.pdf");
    fs::write(&f, "x").unwrap();
    let o = run(home.path(), &["--sort", f.to_str().unwrap()]);
    assert!(o.status.success());
    assert!(f.exists(), "an unapproved rule must not touch files, even on an explicit request");
    assert!(text(&o).contains("awaiting approval"), "{}", text(&o));
}

#[test]
fn once_dry_run_plans_without_touching_anything() {
    let (home, watched) = setup(true);
    let f = watched.path().join("a.pdf");
    fs::write(&f, "x").unwrap();
    // age it so the settle delay does not hide it
    let old = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    filetime::set_file_mtime(&f, old).unwrap();
    let o = run(home.path(), &["--once", "--dry-run"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("[PLAN]"), "{}", text(&o));
    assert!(f.exists());
}

#[test]
fn unknown_flags_and_bad_combinations_are_rejected() {
    let home = TempDir::new().unwrap();
    assert_eq!(run(home.path(), &["--bogus"]).status.code(), Some(2));
    assert_eq!(run(home.path(), &["--dry-run"]).status.code(), Some(2));
}

#[cfg(target_os = "linux")]
#[test]
fn install_and_uninstall_integration_on_linux() {
    let home = TempDir::new().unwrap();
    let script = home.path().join(".local/share/nautilus/scripts/Sort with AutoSort");
    let o = run(home.path(), &["--install-integration"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(script.exists());
    let body = fs::read_to_string(&script).unwrap();
    assert!(body.contains(env!("CARGO_BIN_EXE_autosort")) && body.contains("--sort"));
    let o = run(home.path(), &["--uninstall-integration"]);
    assert!(o.status.success());
    assert!(!script.exists());
}
