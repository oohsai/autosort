use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use autosort_core::*;
use chrono::{DateTime, Duration, Local, TimeZone};
use filetime::{FileTime, set_file_mtime};
use tempfile::TempDir;

fn at(y: i32, m: u32, d: u32) -> DateTime<Local> {
    Local.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
}

fn now() -> DateTime<Local> {
    at(2024, 6, 1)
}

/// Creates `name` in `dir` with content and the given modification time.
fn make(dir: &Path, name: &str, content: &str, modified: DateTime<Local>) -> PathBuf {
    let p = dir.join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, content).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(modified.timestamp(), 0)).unwrap();
    p
}

fn engine() -> Engine {
    Engine::new(5, Arc::new(History::in_memory()))
}

fn folder(dir: &TempDir, rules: Vec<Rule>) -> WatchedFolder {
    let mut f = WatchedFolder::new(dir.path().to_string_lossy());
    f.rules = rules;
    f
}

fn rule(name: &str, conds: Vec<Condition>, actions: Vec<Action>) -> Rule {
    let mut r = Rule::new(name);
    r.conditions = conds;
    r.actions = actions;
    r
}

fn ext_is(e: &str) -> Condition {
    Condition::new(Attribute::Extension, Operator::Equals, e)
}

fn info(name: &str, size: u64, modified: DateTime<Local>) -> FileInfo {
    FileInfo::from_parts(PathBuf::from(name), size, modified, modified)
}

// ------------------------------------------------------------------ matching

#[test]
fn classifies_kinds() {
    assert_eq!(FileKind::classify("JPG"), FileKind::Image);
    assert_eq!(FileKind::classify("pdf"), FileKind::Pdf);
    assert_eq!(FileKind::classify("dmg"), FileKind::Installer);
    assert_eq!(FileKind::classify("zzz"), FileKind::Other);
    assert_eq!(FileKind::from_key(" Image "), Some(FileKind::Image));
}

#[test]
fn extension_list_is_case_and_dot_insensitive() {
    let c = ext_is(" .JPG, png ,heic");
    assert!(c.matches(&info("a.jpg", 1, now()), now()));
    assert!(c.matches(&info("a.PNG", 1, now()), now()));
    assert!(!c.matches(&info("a.gif", 1, now()), now()));
    let not = Condition::new(Attribute::Extension, Operator::NotEquals, "jpg");
    assert!(not.matches(&info("a.gif", 1, now()), now()));
    assert!(!not.matches(&info("a.jpg", 1, now()), now()));
}

#[test]
fn text_and_regex_conditions() {
    let f = info("Invoice_2024-03.pdf", 1, now());
    let m = |op, v: &str| Condition::new(Attribute::Name, op, v).matches(&f, now());
    assert!(m(Operator::Contains, "invoice"));
    assert!(m(Operator::StartsWith, "INVOICE"));
    assert!(m(Operator::EndsWith, ".pdf"));
    assert!(m(Operator::DoesNotContain, "receipt"));
    assert!(m(Operator::MatchesRegex, r"^invoice_\d{4}-\d{2}\.pdf$"));
    assert!(!m(Operator::MatchesRegex, "([unclosed")); // invalid regex never matches
    assert!(!m(Operator::Contains, "")); // empty value never matches "contains"
}

#[test]
fn numeric_conditions() {
    let f = info("big.bin", 5 * 1_048_576, now() - Duration::days(10));
    let size = |op, v: &str| Condition::new(Attribute::SizeMb, op, v).matches(&f, now());
    assert!(size(Operator::GreaterThan, "4"));
    assert!(!size(Operator::GreaterThan, "6"));
    assert!(size(Operator::LessThan, "6"));
    let age = |op, v: &str| Condition::new(Attribute::DaysSinceModified, op, v).matches(&f, now());
    assert!(age(Operator::GreaterThan, "9"));
    assert!(!age(Operator::GreaterThan, "11"));
    assert!(!age(Operator::GreaterThan, "abc")); // garbage number never matches
}

#[test]
fn all_vs_any_and_empty_rule() {
    let f = info("a.pdf", 1, now());
    let mut r = rule("r", vec![ext_is("pdf"), ext_is("png")], vec![]);
    assert!(!r.matches(&f, now())); // all
    r.match_mode = MatchMode::Any;
    assert!(r.matches(&f, now()));
    r.conditions.clear();
    assert!(!r.matches(&f, now())); // an empty rule must not match everything
}

#[test]
fn template_tokens() {
    let f = info("report.pdf", 1, at(2024, 3, 5));
    let out = template::expand("{folder}/{kind}/{year}/{month}-{monthName}/{day}/{date}/{name}.{ext}/{filename}", &f, Path::new("/x"), now());
    assert_eq!(out, "/x/PDFs/2024/03-March/05/2024-03-05/report.pdf/report.pdf");
}

// ------------------------------------------------------------------- actions

#[test]
fn move_with_template_creates_folders() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "report.pdf", "x", at(2024, 3, 5));
    let f = folder(&dir, vec![rule("pdfs", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "{folder}/Sorted/{kind}/{year}")])]);
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 1);
    assert!(entries[0].success, "{:?}", entries[0]);
    assert!(dir.path().join("Sorted/PDFs/2024/report.pdf").exists());
    assert!(!dir.path().join("report.pdf").exists());
}

#[test]
fn relative_destination_is_relative_to_watched_folder() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.png", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("png")], vec![Action::new(ActionKind::Move, "Pics")])]);
    engine().process(&f, false, now());
    assert!(dir.path().join("Pics/a.png").exists());
}

#[test]
fn conflict_keep_both_numbers_the_file() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "new", at(2024, 1, 1));
    make(dir.path(), "out/a.pdf", "old", at(2024, 1, 1));
    make(dir.path(), "out/a 2.pdf", "old2", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "out")])]);
    // `out/` files are not candidates because the folder is non-recursive
    engine().process(&f, false, now());
    assert_eq!(fs::read_to_string(dir.path().join("out/a 3.pdf")).unwrap(), "new");
    assert_eq!(fs::read_to_string(dir.path().join("out/a.pdf")).unwrap(), "old");
}

#[test]
fn conflict_skip_and_overwrite() {
    for (policy, expect_src_exists, expect_content) in [(ConflictPolicy::Skip, true, "old"), (ConflictPolicy::Overwrite, false, "new")] {
        let dir = TempDir::new().unwrap();
        make(dir.path(), "a.pdf", "new", at(2024, 1, 1));
        make(dir.path(), "out/a.pdf", "old", at(2024, 1, 1));
        let mut act = Action::new(ActionKind::Move, "out");
        act.conflict = policy;
        let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![act])]);
        engine().process(&f, false, now());
        assert_eq!(dir.path().join("a.pdf").exists(), expect_src_exists, "{policy:?}");
        assert_eq!(fs::read_to_string(dir.path().join("out/a.pdf")).unwrap(), expect_content, "{policy:?}");
    }
}

#[test]
fn dry_run_touches_nothing_and_remembers_nothing() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "out")])]);
    let e = engine();
    let preview = e.process(&f, true, now());
    assert_eq!(preview.len(), 1);
    assert!(preview[0].dry_run);
    assert!(dir.path().join("a.pdf").exists());
    assert!(!dir.path().join("out").exists());
    // A real run afterwards still acts (dry run must not poison the history).
    assert_eq!(e.process(&f, false, now()).len(), 1);
    assert!(dir.path().join("out/a.pdf").exists());
}

#[test]
fn rename_keeps_extension_and_sanitises() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "report.PDF", "x", at(2024, 3, 5));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Rename, "{date}: {name}")])]);
    engine().process(&f, false, now());
    assert!(dir.path().join("2024-03-05- report.PDF").exists());
}

#[test]
fn actions_chain_on_the_relocated_file() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 3, 5));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![
        Action::new(ActionKind::Move, "out"),
        Action::new(ActionKind::Rename, "{date} {name}"),
    ])]);
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 2);
    assert!(dir.path().join("out/2024-03-05 a.pdf").exists());
}

#[test]
fn copy_runs_once_per_file() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Copy, "backup")])]);
    let e = engine();
    assert_eq!(e.process(&f, false, now()).len(), 1);
    assert_eq!(e.process(&f, false, now()).len(), 0, "history must stop the copy rule looping");
    assert!(dir.path().join("a.pdf").exists());
    assert!(dir.path().join("backup/a.pdf").exists());
    assert!(!dir.path().join("backup/a 2.pdf").exists());
}

#[test]
fn modified_file_is_processed_again() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Copy, "backup")])]);
    let e = engine();
    assert_eq!(e.process(&f, false, now()).len(), 1);
    set_file_mtime(&p, FileTime::from_unix_time(at(2024, 2, 1).timestamp(), 0)).unwrap();
    assert_eq!(e.process(&f, false, now()).len(), 1);
}

#[test]
fn history_persists_across_restarts() {
    let dir = TempDir::new().unwrap();
    let hist_file = dir.path().join("h.json");
    let watched = TempDir::new().unwrap();
    make(watched.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&watched, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Copy, "backup")])]);
    assert_eq!(Engine::new(5, Arc::new(History::persistent(hist_file.clone()))).process(&f, false, now()).len(), 1);
    assert_eq!(Engine::new(5, Arc::new(History::persistent(hist_file))).process(&f, false, now()).len(), 0);
}

#[cfg(unix)]
#[test]
fn run_command_gets_the_file_path() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![
        Action::new(ActionKind::RunCommand, "cp \"$1\" \"$1.bak\" && cp \"$AUTOSORT_FILE\" \"$AUTOSORT_FILE.env\""),
    ])]);
    let entries = engine().process(&f, false, now());
    assert!(entries[0].success, "{:?}", entries[0]);
    assert!(dir.path().join("a.pdf.bak").exists());
    assert!(dir.path().join("a.pdf.env").exists());
}

#[cfg(unix)]
#[test]
fn failing_command_is_reported_and_stops_the_rule() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![
        Action::new(ActionKind::RunCommand, "exit 3"),
        Action::new(ActionKind::Move, "out"),
    ])]);
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].success);
    assert!(dir.path().join("a.pdf").exists(), "the move after a failed step must not run");
}

#[test]
fn missing_parameter_is_an_error_not_a_crash() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "  ")])]);
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].success);
    assert!(dir.path().join("a.pdf").exists());
}

// -------------------------------------------------------------------- engine

#[test]
fn first_matching_rule_wins_unless_stop_is_off() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 3, 5));
    let r1 = rule("one", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "one")]);
    let r2 = rule("two", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "two")]);
    let f = folder(&dir, vec![r1.clone(), r2]);
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 1);
    assert!(dir.path().join("one/a.pdf").exists());

    // With stop_processing off the second rule sees the file at its new location.
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 3, 5));
    let mut r1 = r1;
    r1.stop_processing = false;
    let r2 = rule("two", vec![ext_is("pdf")], vec![Action::new(ActionKind::Rename, "renamed")]);
    let f = folder(&dir, vec![r1, r2]);
    engine().process(&f, false, now());
    assert!(dir.path().join("one/renamed.pdf").exists());
}

#[test]
fn disabled_and_empty_rules_are_ignored() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let mut disabled = rule("d", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "d")]);
    disabled.enabled = false;
    let no_conditions = rule("n", vec![], vec![Action::new(ActionKind::Move, "n")]);
    let f = folder(&dir, vec![disabled, no_conditions]);
    assert!(engine().process(&f, false, now()).is_empty());
    assert!(dir.path().join("a.pdf").exists());
}

#[test]
fn recent_files_hidden_files_and_partial_downloads_are_skipped() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "fresh.pdf", "x", now() - Duration::seconds(1));
    make(dir.path(), ".hidden.pdf", "x", at(2024, 1, 1));
    make(dir.path(), "movie.mp4.crdownload", "x", at(2024, 1, 1));
    make(dir.path(), "~$doc.docx", "x", at(2024, 1, 1));
    make(dir.path(), "ok.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("any", vec![Condition::new(Attribute::Name, Operator::DoesNotContain, "zzz")], vec![Action::new(ActionKind::Move, "out")])]);
    engine().process(&f, false, now());
    assert!(dir.path().join("out/ok.pdf").exists());
    assert!(dir.path().join("fresh.pdf").exists());
    assert!(dir.path().join(".hidden.pdf").exists());
    assert!(dir.path().join("movie.mp4.crdownload").exists());
    assert!(dir.path().join("~$doc.docx").exists());
}

#[test]
fn subfolders_only_when_enabled_and_second_run_is_a_noop() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "top.pdf", "x", at(2024, 1, 1));
    make(dir.path(), "deep/inner.pdf", "x", at(2024, 1, 1));
    let rules = vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "{folder}/Sorted/{kind}")])];

    let mut f = folder(&dir, rules);
    assert_eq!(engine().process(&f, true, now()).len(), 1, "non-recursive sees only top-level files");

    f.include_subfolders = true;
    let e = engine();
    assert_eq!(e.process(&f, false, now()).len(), 2);
    assert!(dir.path().join("Sorted/PDFs/top.pdf").exists());
    assert!(dir.path().join("Sorted/PDFs/inner.pdf").exists());
    assert_eq!(e.process(&f, false, now()).len(), 0, "already-sorted files must not be touched again");
}

#[test]
fn already_in_place_is_not_renamed_with_a_counter() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "Sorted/PDFs/a.pdf", "x", at(2024, 1, 1));
    let mut f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "{folder}/Sorted/{kind}")])]);
    f.include_subfolders = true;
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].detail, "Already in place");
    assert!(!dir.path().join("Sorted/PDFs/a 2.pdf").exists());
}

#[test]
fn starter_rules_sort_a_messy_downloads_folder() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "photo.jpg", "x", at(2024, 1, 1));
    make(dir.path(), "Screenshot 2024-03-01.png", "x", at(2024, 3, 1));
    make(dir.path(), "notes.txt", "x", at(2024, 1, 1));
    make(dir.path(), "mystery.zzz", "x", at(2024, 1, 1));
    let mut f = folder(&dir, presets::downloads_starter_rules());
    f.rules.retain(|r| r.name != "Trash old installers"); // trash is not available in CI sandboxes
    engine().process(&f, false, now());
    assert!(dir.path().join("Sorted/Images/photo.jpg").exists());
    assert!(dir.path().join("Sorted/Screenshots/2024-03/Screenshot 2024-03-01.png").exists());
    assert!(dir.path().join("Sorted/Documents/notes.txt").exists());
    assert!(dir.path().join("mystery.zzz").exists(), "unknown kinds stay put");
}

// --------------------------------------------------------------------- store

#[test]
fn config_roundtrip_and_forward_compat() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.json");
    let mut cfg = AppConfig::default();
    let mut f = WatchedFolder::new("~/Downloads");
    f.rules = presets::downloads_starter_rules();
    cfg.folders.push(f);
    store::save_json(&path, &cfg).unwrap();
    assert_eq!(store::load_json::<AppConfig>(&path).unwrap(), cfg);

    // Minimal/older config files still load, with defaults filled in.
    fs::write(&path, r#"{"folders":[{"path":"/tmp/x","rules":[{"name":"r"}]}]}"#).unwrap();
    let loaded: AppConfig = store::load_json(&path).unwrap();
    assert_eq!(loaded.settle_seconds, 5);
    assert!(loaded.folders[0].enabled);
    assert!(loaded.folders[0].rules[0].enabled && loaded.folders[0].rules[0].stop_processing);
}

#[test]
fn instance_lock_is_exclusive() {
    let dir = TempDir::new().unwrap();
    let paths = store::Paths { dir: dir.path().to_path_buf() };
    let first = store::InstanceLock::acquire(&paths).expect("first lock");
    assert!(store::InstanceLock::acquire(&paths).is_none(), "second instance must be refused");
    drop(first);
    assert!(store::InstanceLock::acquire(&paths).is_some());
}

// ------------------------------------------------------------------- service

#[test]
fn service_reacts_to_new_files() {
    use std::sync::Mutex;
    use std::time::{Duration as StdDuration, Instant};

    let dir = TempDir::new().unwrap();
    let mut f = WatchedFolder::new(dir.path().to_string_lossy());
    f.rules = vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "out")])];
    let config = Arc::new(Mutex::new(AppConfig { folders: vec![f], settle_seconds: 0, ..Default::default() }));
    let log: Arc<Mutex<Vec<ActivityEntry>>> = Arc::default();
    let sink_log = log.clone();
    let service = Service::start(config, Arc::new(History::in_memory()), Arc::new(move |e| sink_log.lock().unwrap().extend(e)));
    std::thread::sleep(StdDuration::from_millis(400)); // let the watcher attach

    fs::write(dir.path().join("late.pdf"), "x").unwrap();
    let deadline = Instant::now() + StdDuration::from_secs(10);
    while !dir.path().join("out/late.pdf").exists() && Instant::now() < deadline {
        std::thread::sleep(StdDuration::from_millis(100));
    }
    assert!(dir.path().join("out/late.pdf").exists(), "file watcher + debounce should have sorted the new file");
    assert_eq!(log.lock().unwrap().len(), 1);

    // Pausing stops processing.
    service.set_paused(true);
    std::thread::sleep(StdDuration::from_millis(300));
    fs::write(dir.path().join("paused.pdf"), "x").unwrap();
    std::thread::sleep(StdDuration::from_millis(2500));
    assert!(dir.path().join("paused.pdf").exists());
    service.set_paused(false); // resuming sweeps immediately
    let deadline = Instant::now() + StdDuration::from_secs(10);
    while !dir.path().join("out/paused.pdf").exists() && Instant::now() < deadline {
        std::thread::sleep(StdDuration::from_millis(100));
    }
    assert!(dir.path().join("out/paused.pdf").exists());
}

// ---------------------------------------------------------------------- undo

fn run_logged(f: &WatchedFolder, history: Arc<History>) -> (store::ActivityLog, Vec<ActivityEntry>) {
    let log = store::ActivityLog::new(None);
    let entries = Engine::new(5, history).process(f, false, now());
    log.push(entries.clone());
    (log, entries)
}

#[test]
fn undo_move_restores_file_and_it_is_not_resorted() {
    let dir = TempDir::new().unwrap();
    let orig = make(dir.path(), "a.pdf", "data", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "out")])]);
    let history = Arc::new(History::in_memory());
    let (log, entries) = run_logged(&f, history.clone());
    assert!(!orig.exists());

    let msg = log.undo(entries[0].id, &history).expect("undo works");
    assert!(msg.starts_with("Restored"), "{msg}");
    assert_eq!(fs::read_to_string(&orig).unwrap(), "data");
    assert!(!dir.path().join("out/a.pdf").exists());

    // The restored file stays where the user put it...
    assert!(Engine::new(5, history.clone()).process(&f, false, now()).is_empty());
    assert!(orig.exists());
    // ...and the log remembers it, so a second undo is refused.
    assert!(log.undo(entries[0].id, &history).unwrap_err().contains("already undone"));
    assert!(log.snapshot()[0].undone);
}

#[test]
fn undo_rename_and_chained_steps_in_reverse_order() {
    let dir = TempDir::new().unwrap();
    let orig = make(dir.path(), "a.pdf", "x", at(2024, 3, 5));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![
        Action::new(ActionKind::Move, "out"),
        Action::new(ActionKind::Rename, "{date} {name}"),
    ])]);
    let history = Arc::new(History::in_memory());
    let (log, entries) = run_logged(&f, history.clone());
    assert_eq!(entries.len(), 2);

    // Undoing the first step while the second is still applied must fail safely.
    let err = log.undo(entries[0].id, &history).unwrap_err();
    assert!(err.contains("undo those first"), "{err}");
    assert!(dir.path().join("out/2024-03-05 a.pdf").exists(), "nothing was touched");

    log.undo(entries[1].id, &history).unwrap();
    assert!(dir.path().join("out/a.pdf").exists());
    log.undo(entries[0].id, &history).unwrap();
    assert!(orig.exists());
}

#[test]
fn undo_refuses_to_overwrite_and_dry_runs_have_no_undo() {
    let dir = TempDir::new().unwrap();
    let orig = make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "out")])]);

    let preview = Engine::new(5, Arc::new(History::in_memory())).process(&f, true, now());
    assert!(preview[0].undo.is_none());

    let history = Arc::new(History::in_memory());
    let (log, entries) = run_logged(&f, history.clone());
    fs::write(&orig, "someone else's file").unwrap(); // the original spot got taken again
    let err = log.undo(entries[0].id, &history).unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(fs::read_to_string(&orig).unwrap(), "someone else's file");
    assert!(dir.path().join("out/a.pdf").exists());
}

#[test]
fn skipped_and_in_place_steps_are_not_undoable_and_copy_undo_handles_missing_copy() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "Sorted/PDFs/a.pdf", "x", at(2024, 1, 1));
    let mut f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "{folder}/Sorted/{kind}")])]);
    f.include_subfolders = true;
    let (_, entries) = run_logged(&f, Arc::new(History::in_memory()));
    assert_eq!(entries[0].detail, "Already in place");
    assert!(entries[0].undo.is_none());

    let op = UndoOp::Copied { copy: dir.path().join("nope.pdf") };
    assert!(undo::apply(&op, &History::in_memory()).unwrap_err().contains("already gone"));
}

#[test]
fn old_activity_logs_without_undo_fields_still_load() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("activity.json");
    fs::write(&path, r#"[{"time":"2024-03-05T12:00:00+00:00","folder":"f","rule":"r","file":"a.pdf","action":"Move","detail":"d","success":true,"dry_run":false}]"#).unwrap();
    let log = store::ActivityLog::new(Some(path));
    let e = &log.snapshot()[0];
    assert!(e.undo.is_none() && !e.undone);
}

// ------------------------------------------------------------------ summaries

#[test]
fn rule_summaries_read_like_english() {
    let mut r = rule("r", vec![ext_is("pdf"), Condition::new(Attribute::Kind, Operator::NotEquals, "other")], vec![
        Action::new(ActionKind::Move, "~/Docs"),
        Action::new(ActionKind::Trash, ""),
    ]);
    assert_eq!(r.summary(), "If extension is pdf and kind is not Other, then move to ~/Docs, then move to Trash");
    r.match_mode = MatchMode::Any;
    assert!(r.summary().contains(" or "));
    r.actions.clear();
    assert!(r.summary().ends_with("then nothing yet"));
    r.conditions.clear();
    assert!(r.summary().starts_with("No conditions"));
    assert_eq!(Condition::default().summary(), "extension is …");
}
