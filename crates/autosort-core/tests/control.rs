//! Ignore lists, learning mode, schedules, stats, history semantics and rule import/export.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use autosort_core::*;
use chrono::{DateTime, Duration, Local, TimeZone};
use filetime::{FileTime, set_file_mtime};
use tempfile::TempDir;

fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
    Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
}
fn day(y: i32, m: u32, d: u32) -> DateTime<Local> {
    at(y, m, d, 12, 0)
}
fn now() -> DateTime<Local> {
    day(2024, 6, 1)
}
fn make(dir: &Path, name: &str, content: &str, modified: DateTime<Local>) -> PathBuf {
    let p = dir.join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, content).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(modified.timestamp(), 0)).unwrap();
    p
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
fn mv(to: &str) -> Action {
    Action::new(ActionKind::Move, to)
}
fn engine_with(stats: &Arc<Stats>) -> Engine {
    Engine::new(5, Arc::new(History::in_memory())).with_stats(stats.clone())
}
fn engine() -> Engine {
    engine_with(&Arc::new(Stats::in_memory()))
}

// ------------------------------------------------------------------------ ignore lists

#[test]
fn ignore_patterns_by_name_extension_and_folder() {
    let dir = TempDir::new().unwrap();
    for n in ["keep.pdf", "draft.psd", "Notes.TXT", "scratch-1.pdf", "Archive/old.pdf", "deep/Archive/older.pdf", "ok/inner.pdf"] {
        make(dir.path(), n, "x", day(2024, 1, 1));
    }
    let mut f = folder(&dir, vec![rule("all", vec![Condition::new(Attribute::Name, Operator::DoesNotContain, "zzz")], vec![mv("{folder}/Out")])]);
    f.include_subfolders = true;
    f.ignore = vec!["*.psd".into(), "notes.txt".into(), "scratch-*".into(), "Archive".into(), "ok/".into()];
    let entries = engine().process(&f, false, now());
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(dir.path().join("Out/keep.pdf").exists());
    for stays in ["draft.psd", "Notes.TXT", "scratch-1.pdf", "Archive/old.pdf", "deep/Archive/older.pdf", "ok/inner.pdf"] {
        assert!(dir.path().join(stays).exists(), "{stays} must be ignored");
    }
}

#[test]
fn ignore_path_patterns_and_blank_or_invalid_entries() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a/b/skip.pdf", "x", day(2024, 1, 1));
    make(dir.path(), "a/take.pdf", "x", day(2024, 1, 1));
    let mut f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![mv("{folder}/Out")])]);
    f.include_subfolders = true;
    f.ignore = vec!["a/b/*".into(), "   ".into(), "[unclosed".into(), "".into()];
    engine().process(&f, false, now());
    assert!(dir.path().join("a/b/skip.pdf").exists());
    assert!(dir.path().join("Out/take.pdf").exists());
}

#[test]
fn ignore_list_is_respected_for_explicit_file_runs_too() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "secret.pdf", "x", day(2024, 1, 1));
    let mut f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![mv("{folder}/Out")])]);
    f.ignore = vec!["secret*".into()];
    let opts = RunOptions { only: Some(vec![p.clone()]), ignore_history: true, ignore_settle: true, ..Default::default() };
    assert!(engine().process_with(&f, &opts, now()).is_empty());
    assert!(p.exists());
}

// ------------------------------------------------------------------------ learning mode

#[test]
fn unapproved_rules_only_preview() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", day(2024, 1, 1));
    let mut r = rule("new rule", vec![ext_is("pdf")], vec![mv("{folder}/Out")]);
    r.approved = false;
    let f = folder(&dir, vec![r]);

    // a real run (what the background service does) never touches files
    assert!(engine().process(&f, false, now()).is_empty());
    assert!(dir.path().join("a.pdf").exists());

    // a preview shows it, clearly labelled, and still changes nothing
    let preview = engine().process(&f, true, now());
    assert_eq!(preview.len(), 1);
    assert!(preview[0].rule.contains("awaiting approval"), "{}", preview[0].rule);
    assert!(dir.path().join("a.pdf").exists());

    // once approved it acts
    let mut f = f;
    f.rules[0].approved = true;
    assert_eq!(engine().process(&f, false, now()).len(), 1);
    assert!(dir.path().join("Out/a.pdf").exists());
}

#[test]
fn legacy_configs_default_to_approved_and_import_respects_learning_mode() {
    let loaded: AppConfig = serde_json::from_str(r#"{"folders":[{"path":"/x","rules":[{"name":"old"}]}]}"#).unwrap();
    assert!(loaded.folders[0].rules[0].approved, "existing rules keep working after an upgrade");
    assert!(loaded.learning_mode, "learning mode is on by default");
    assert!(loaded.folders[0].ignore.is_empty());
    assert_eq!(loaded.folders[0].rules[0].schedule, Schedule::Always);
}

// ------------------------------------------------------------------------------ schedules

#[test]
fn schedule_occurrences() {
    let daily = Schedule::Daily { hour: 9, minute: 30 };
    assert_eq!(daily.latest_occurrence(at(2024, 6, 1, 10, 0)), Some(at(2024, 6, 1, 9, 30)));
    assert_eq!(daily.latest_occurrence(at(2024, 6, 1, 9, 0)), Some(at(2024, 5, 31, 9, 30)), "before today's time -> yesterday's");
    assert_eq!(daily.latest_occurrence(at(2024, 6, 1, 9, 30)), Some(at(2024, 6, 1, 9, 30)), "exactly on the minute counts");

    // 2024-06-01 is a Saturday. Weekly on Sunday (6) at 08:00 -> the previous Sunday, May 26.
    let weekly = Schedule::Weekly { weekday: 6, hour: 8, minute: 0 };
    assert_eq!(weekly.latest_occurrence(at(2024, 6, 1, 12, 0)), Some(at(2024, 5, 26, 8, 0)));
    assert_eq!(weekly.latest_occurrence(at(2024, 6, 2, 7, 0)), Some(at(2024, 5, 26, 8, 0)), "Sunday but before 08:00");
    assert_eq!(weekly.latest_occurrence(at(2024, 6, 2, 8, 0)), Some(at(2024, 6, 2, 8, 0)));
    assert_eq!(Schedule::Always.latest_occurrence(now()), None);

    assert_eq!(daily.summary(), "daily at 09:30");
    assert_eq!(weekly.summary(), "every Sunday at 08:00");
}

#[test]
fn scheduled_rule_waits_for_its_time_then_runs_once() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "old.log", "x", day(2024, 1, 1));
    let mut r = rule("weekly cleanup", vec![ext_is("log")], vec![mv("{folder}/Archive")]);
    r.schedule = Schedule::Daily { hour: 9, minute: 0 };
    let f = folder(&dir, vec![r]);
    let stats = Arc::new(Stats::in_memory());
    let e = engine_with(&stats);

    // First sight at 15:00 only starts the clock: a new "daily at 09:00" rule must not fire immediately.
    assert!(e.process_with(&f, &RunOptions::default(), at(2024, 6, 1, 15, 0)).is_empty());
    assert!(dir.path().join("old.log").exists());
    // Later the same day: still not due.
    assert!(e.process_with(&f, &RunOptions::default(), at(2024, 6, 1, 23, 0)).is_empty());
    // After 09:00 the next day it is due and runs...
    assert_eq!(e.process_with(&f, &RunOptions::default(), at(2024, 6, 2, 9, 5)).len(), 1);
    assert!(dir.path().join("Archive/old.log").exists());
    // ...and does not run again until the following 09:00.
    make(dir.path(), "new.log", "x", day(2024, 1, 2));
    assert!(e.process_with(&f, &RunOptions::default(), at(2024, 6, 2, 18, 0)).is_empty());
    assert_eq!(e.process_with(&f, &RunOptions::default(), at(2024, 6, 3, 9, 1)).len(), 1);
    assert!(dir.path().join("Archive/new.log").exists());
}

#[test]
fn run_now_forces_scheduled_rules_and_resets_their_clock() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.log", "x", day(2024, 1, 1));
    let mut r = rule("nightly", vec![ext_is("log")], vec![mv("{folder}/Archive")]);
    r.schedule = Schedule::Daily { hour: 3, minute: 0 };
    let rid = r.id;
    let f = folder(&dir, vec![r]);
    let stats = Arc::new(Stats::in_memory());
    let e = engine_with(&stats);
    let forced = RunOptions { force_scheduled: true, ..Default::default() };
    assert_eq!(e.process_with(&f, &forced, at(2024, 6, 1, 15, 0)).len(), 1);
    assert_eq!(stats.last_scheduled_run(rid), Some(at(2024, 6, 1, 15, 0)));
    // a later normal sweep the same day finds it not due
    make(dir.path(), "b.log", "x", day(2024, 1, 1));
    assert!(e.process_with(&f, &RunOptions::default(), at(2024, 6, 1, 16, 0)).is_empty());
}

#[test]
fn preview_includes_scheduled_rules_without_starting_their_clock() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.log", "x", day(2024, 1, 1));
    let mut r = rule("nightly", vec![ext_is("log")], vec![mv("{folder}/Archive")]);
    r.schedule = Schedule::Daily { hour: 3, minute: 0 };
    let rid = r.id;
    let f = folder(&dir, vec![r]);
    let stats = Arc::new(Stats::in_memory());
    assert_eq!(engine_with(&stats).process(&f, true, now()).len(), 1);
    assert_eq!(stats.last_scheduled_run(rid), None);
    assert!(dir.path().join("a.log").exists());
}

// ---------------------------------------------------------------------------------- stats

#[test]
fn stats_count_files_and_actions_per_rule() {
    let dir = TempDir::new().unwrap();
    for n in ["a.pdf", "b.pdf", "c.txt"] {
        make(dir.path(), n, "x", day(2024, 1, 1));
    }
    let pdf = rule("pdfs", vec![ext_is("pdf")], vec![Action::new(ActionKind::Rename, "{date} {name}"), mv("{folder}/PDFs")]);
    let txt = rule("txt", vec![ext_is("txt")], vec![mv("{folder}/Text")]);
    let (pid, tid) = (pdf.id, txt.id);
    let stats = Arc::new(Stats::in_memory());
    engine_with(&stats).process(&folder(&dir, vec![pdf, txt]), false, now());
    let p = stats.get(pid);
    assert_eq!((p.files, p.actions), (2, 4));
    assert_eq!(p.last_match, Some(now()));
    assert_eq!((stats.get(tid).files, stats.get(tid).actions), (1, 1));
    assert_eq!(stats.get(Uuid_new()).files, 0);
}

#[allow(non_snake_case)]
fn Uuid_new() -> uuid::Uuid {
    uuid::Uuid::new_v4()
}

#[test]
fn stats_persist_and_failed_actions_do_not_count() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("stats.json");
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", day(2024, 1, 1));
    let good = rule("good", vec![ext_is("pdf")], vec![Action::new(ActionKind::Copy, "{folder}/Backup")]);
    let bad = rule("bad", vec![ext_is("pdf")], vec![Action::new(ActionKind::Move, "  ")]);
    let (gid, bid) = (good.id, bad.id);
    let mut good = good;
    good.stop_processing = false;
    let stats = Arc::new(Stats::persistent(file.clone()));
    engine_with(&stats).process(&folder(&dir, vec![good, bad]), false, now());
    let reloaded = Stats::persistent(file);
    assert_eq!(reloaded.get(gid).files, 1);
    assert_eq!(reloaded.get(bid).files, 0, "a rule whose only action failed handled nothing");
}

// ------------------------------------------------------------------- per-rule history

#[test]
fn a_later_rule_can_still_look_at_a_file_an_earlier_rule_handled() {
    let dir = TempDir::new().unwrap();
    // filed long ago by "sort"; a separate cleanup rule must still be able to see it
    make(dir.path(), "Sorted/old.pdf", "x", day(2023, 1, 1));
    let sort = rule("sort", vec![ext_is("pdf")], vec![mv("{folder}/Sorted")]);
    let mut cleanup = rule(
        "archive after 90 days",
        vec![ext_is("pdf"), Condition::new(Attribute::DaysSinceModified, Operator::GreaterThan, "90")],
        vec![mv("{folder}/Archive")],
    );
    cleanup.stop_processing = true;
    let mut f = folder(&dir, vec![cleanup, sort]);
    f.include_subfolders = true;
    let e = engine();
    // a fresh file is sorted by "sort" first
    make(dir.path(), "new.pdf", "y", now() - Duration::days(1));
    e.process(&f, false, now());
    assert!(dir.path().join("Sorted/new.pdf").exists());
    // ...and the cleanup rule archived the old one (the old file had no history from "sort")
    assert!(dir.path().join("Archive/old.pdf").exists());
}

#[test]
fn rename_rule_does_not_re_fire_on_its_own_output_across_runs() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", day(2024, 1, 1));
    let mut ren = rule("prefix", vec![ext_is("pdf")], vec![Action::new(ActionKind::Rename, "{date} {name}")]);
    ren.stop_processing = false;
    let sort = rule("sort", vec![ext_is("pdf")], vec![mv("{folder}/Sorted")]);
    let mut f = folder(&dir, vec![ren, sort]);
    f.include_subfolders = true;
    let e = engine();
    for _ in 0..4 {
        e.process(&f, false, now());
    }
    assert!(dir.path().join("Sorted/2024-01-01 a.pdf").exists());
    assert_eq!(fs::read_dir(dir.path().join("Sorted")).unwrap().count(), 1, "exactly one file, never re-prefixed");
}

// ----------------------------------------------------------------------- explicit file runs

#[test]
fn explicit_runs_can_ignore_history_and_settling() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "a.pdf", "x", now() - Duration::seconds(1)); // brand new
    let f = folder(&dir, vec![rule("r", vec![ext_is("pdf")], vec![Action::new(ActionKind::Copy, "{folder}/Backup")])]);
    let e = engine();
    assert!(e.process(&f, false, now()).is_empty(), "too fresh for the background sweep");
    let opts = RunOptions { only: Some(vec![p]), ignore_history: true, ignore_settle: true, ..Default::default() };
    assert_eq!(e.process_with(&f, &opts, now()).len(), 1);
    assert!(dir.path().join("Backup/a.pdf").exists());
    // explicit run again: history is ignored, so it acts again (keeping both copies)
    assert_eq!(e.process_with(&f, &opts, now()).len(), 1);
    assert!(dir.path().join("Backup/a 2.pdf").exists());
}

// ------------------------------------------------------------------------ import / export

#[test]
fn export_import_round_trip_with_fresh_ids() {
    let mut r = rule("PDFs", vec![ext_is("pdf"), Condition::new(Attribute::WhereFrom, Operator::Contains, "github")], vec![mv("~/Docs/{year}")]);
    r.schedule = Schedule::Weekly { weekday: 2, hour: 7, minute: 15 };
    r.stop_processing = false;
    let json = rules_io::export_rules(std::slice::from_ref(&r));
    assert!(json.contains("autosort_rules"));

    let back = rules_io::import_rules(&json, false).unwrap();
    assert_eq!(back.len(), 1);
    let b = &back[0];
    assert_ne!(b.id, r.id);
    assert_ne!(b.conditions[0].id, r.conditions[0].id);
    assert_ne!(b.actions[0].id, r.actions[0].id);
    assert_eq!(b.name, r.name);
    assert_eq!(b.schedule, r.schedule);
    assert_eq!(b.conditions[1].attribute, Attribute::WhereFrom);
    assert!(!b.stop_processing);
    assert!(b.approved);

    // learning mode: imports must be approved by the person before they can touch files
    assert!(!rules_io::import_rules(&json, true).unwrap()[0].approved);
}

#[test]
fn import_rejects_garbage_and_newer_formats() {
    assert!(rules_io::import_rules("not json", false).is_err());
    assert!(rules_io::import_rules(r#"{"rules":[]}"#, false).is_err(), "missing marker");
    assert!(rules_io::import_rules(r#"{"autosort_rules":99,"rules":[]}"#, false).unwrap_err().contains("newer"));
    assert_eq!(rules_io::import_rules(r#"{"autosort_rules":1,"rules":[]}"#, false).unwrap().len(), 0);
}
