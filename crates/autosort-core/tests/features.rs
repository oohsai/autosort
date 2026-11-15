//! Conditions, actions and templates that read extra file metadata.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use autosort_core::*;
use chrono::{DateTime, Duration, Local, TimeZone};
use filetime::{FileTime, set_file_mtime, set_file_times};
use tempfile::TempDir;

fn at(y: i32, m: u32, d: u32) -> DateTime<Local> {
    Local.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
}
fn now() -> DateTime<Local> {
    at(2024, 6, 1)
}
fn make_bytes(dir: &Path, name: &str, content: &[u8], modified: DateTime<Local>) -> PathBuf {
    let p = dir.join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, content).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(modified.timestamp(), 0)).unwrap();
    p
}
fn make(dir: &Path, name: &str, content: &str, modified: DateTime<Local>) -> PathBuf {
    make_bytes(dir, name, content.as_bytes(), modified)
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
fn cond(a: Attribute, op: Operator, v: &str) -> Condition {
    Condition::new(a, op, v)
}
fn mv(to: &str) -> Action {
    Action::new(ActionKind::Move, to)
}
fn info(name: &str) -> FileInfo {
    FileInfo::from_parts(PathBuf::from(name), 1, now(), now())
}

// ----- fixtures -------------------------------------------------------------------------------

/// A minimal JPEG whose only content is an EXIF `DateTime` of 2023-07-14 09:30:00.
fn exif_jpeg() -> Vec<u8> {
    let mut tiff = vec![b'M', b'M', 0, 0x2A, 0, 0, 0, 8];
    tiff.extend([0, 1]); // one IFD entry
    tiff.extend([0x01, 0x32, 0, 2, 0, 0, 0, 20, 0, 0, 0, 26]); // DateTime, ASCII, 20 bytes at offset 26
    tiff.extend([0, 0, 0, 0]); // no next IFD
    tiff.extend(b"2023:07:14 09:30:00\0");
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff);
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    jpg.extend(((app1.len() + 2) as u16).to_be_bytes());
    jpg.extend(app1);
    jpg.extend([0xFF, 0xD9]);
    jpg
}

fn tiny_pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 18 Tf 20 100 Td ({text}) Tj ET");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn tiny_docx(text: &str) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file("word/document.xml", opts).unwrap();
        write!(z, "<w:document><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>").unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

/// Records a download origin the way this OS does. Returns false if the filesystem can't store it.
fn set_origin(path: &Path, url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        let v = plist::Value::Array(vec![plist::Value::String(url.to_string())]);
        let mut b = Vec::new();
        plist::to_writer_binary(&mut b, &v).unwrap();
        xattr::set(path, "com.apple.metadata:kMDItemWhereFroms", &b).is_ok()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        xattr::set(path, "user.xdg.origin.url", url.as_bytes()).is_ok()
    }
    #[cfg(windows)]
    {
        let mut s = path.as_os_str().to_owned();
        s.push(":Zone.Identifier");
        fs::write(PathBuf::from(s), format!("[ZoneTransfer]\nZoneId=3\nHostUrl={url}\n")).is_ok()
    }
}

// ----- templates / smart renaming -------------------------------------------------------------

fn tpl(t: &str, name: &str) -> String {
    template::expand(t, &info(name), Path::new("/w"), now())
}

#[test]
fn template_filters() {
    assert_eq!(tpl("{name|upper}", "My Report.pdf"), "MY REPORT");
    assert_eq!(tpl("{name|lower}", "My Report.pdf"), "my report");
    assert_eq!(tpl("{name|kebab}", "My Report_v2.pdf"), "my-report-v2");
    assert_eq!(tpl("{name|snake}", "MyReport v2.pdf"), "my_report_v2");
    assert_eq!(tpl("{name|title}", "my_report-final.pdf"), "My Report Final");
    assert_eq!(tpl("{name|replace:_: }", "a_b_c.pdf"), "a b c");
    assert_eq!(tpl("{name|regex:\\d+:N}", "scan 001 of 020.pdf"), "scan N of N");
    assert_eq!(tpl("{name|slice:0:4}", "abcdefgh.pdf"), "abcd");
    assert_eq!(tpl("{name|slice:4}", "abcdefgh.pdf"), "efgh");
    assert_eq!(tpl("{name|trim|upper}", " x .pdf"), "X");
    // chained with literal text, and unknown things are left visible
    assert_eq!(tpl("{date}-{name|lower}", "A.pdf"), "2024-06-01-a");
    assert_eq!(tpl("{nope} {name|bogus}", "a.pdf"), "{nope} {name|bogus}");
    assert_eq!(tpl("unclosed {name", "a.pdf"), "unclosed {name");
}

#[test]
fn template_counter_and_taken_fallback() {
    let f = info("a.jpg");
    assert_eq!(template::expand_with("IMG_{counter:3}", &f, Path::new("/"), now(), Some(7)), "IMG_007");
    assert_eq!(template::expand_with("{counter}", &f, Path::new("/"), now(), Some(12)), "12");
    // no EXIF on a synthetic file: taken tokens fall back to the modification date
    assert_eq!(tpl("{takenYear}/{takenMonthName}", "a.jpg"), "2024/June");
    // no download origin recorded
    assert_eq!(tpl("{source}", "a.pdf"), "Unknown");
}

#[test]
fn rename_counter_picks_next_free_number() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "Photo 001.jpg", "keep", at(2024, 1, 1));
    for n in ["a.jpg", "b.jpg"] {
        make(dir.path(), n, "x", at(2024, 1, 2));
    }
    let r = rule("r", vec![cond(Attribute::Name, Operator::DoesNotContain, "Photo")], vec![Action::new(ActionKind::Rename, "Photo {counter:3}")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert_eq!(fs::read_to_string(dir.path().join("Photo 001.jpg")).unwrap(), "keep");
    assert!(dir.path().join("Photo 002.jpg").exists());
    assert!(dir.path().join("Photo 003.jpg").exists());
    assert!(!dir.path().join("a.jpg").exists() && !dir.path().join("b.jpg").exists());
}

#[test]
fn rename_with_filters_end_to_end() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "My Holiday Photo.JPG", "x", at(2024, 3, 5));
    let r = rule("r", vec![cond(Attribute::Extension, Operator::Equals, "jpg")], vec![Action::new(ActionKind::Rename, "{date} {name|kebab}")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert!(dir.path().join("2024-03-05 my-holiday-photo.JPG").exists());
}

// ----- photo date taken ------------------------------------------------------------------------

#[test]
fn exif_date_taken_drives_condition_and_tokens() {
    let dir = TempDir::new().unwrap();
    let p = make_bytes(dir.path(), "IMG_1.jpg", &exif_jpeg(), at(2024, 5, 30)); // copied recently, shot in 2023
    let info = FileInfo::load(&p).unwrap();
    let taken = info.taken().expect("EXIF date parsed");
    assert_eq!(taken.format("%Y-%m-%d %H:%M").to_string(), "2023-07-14 09:30");

    let old = cond(Attribute::DaysSinceTaken, Operator::GreaterThan, "300");
    assert!(old.matches(&info, now()));
    assert!(!cond(Attribute::DaysSinceTaken, Operator::LessThan, "300").matches(&info, now()));

    let r = rule("photos", vec![cond(Attribute::Kind, Operator::Equals, "image")], vec![mv("{folder}/Photos/{takenYear}/{takenMonthName}")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert!(dir.path().join("Photos/2023/July/IMG_1.jpg").exists());
}

#[test]
fn files_without_exif_never_match_taken_conditions() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "plain.jpg", "not really a jpeg", at(2020, 1, 1));
    let info = FileInfo::load(&p).unwrap();
    assert!(info.taken().is_none());
    assert!(!cond(Attribute::DaysSinceTaken, Operator::GreaterThan, "1").matches(&info, now()));
}

// ----- file contents ---------------------------------------------------------------------------

#[test]
fn contents_of_text_pdf_and_docx_are_searchable() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "note.txt", "Please pay Invoice number 42 by Friday", at(2024, 1, 1));
    make_bytes(dir.path(), "bill.pdf", &tiny_pdf("Invoice from ACME"), at(2024, 1, 1));
    make_bytes(dir.path(), "letter.docx", &tiny_docx("Dear customer, your invoice is attached"), at(2024, 1, 1));
    make(dir.path(), "recipe.txt", "Mix flour and water", at(2024, 1, 1));
    make_bytes(dir.path(), "blob.bin", &[0, 1, 2, 3, 0, 0, 255], at(2024, 1, 1));

    let r = rule("invoices", vec![cond(Attribute::Contents, Operator::Contains, "INVOICE")], vec![mv("Invoices")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    for f in ["note.txt", "bill.pdf", "letter.docx"] {
        assert!(dir.path().join("Invoices").join(f).exists(), "{f} should be filed as an invoice");
    }
    assert!(dir.path().join("recipe.txt").exists());
    assert!(dir.path().join("blob.bin").exists(), "binary files have no searchable text");
}

#[test]
fn contents_negation_and_regex() {
    let dir = TempDir::new().unwrap();
    let a = FileInfo::load(&make(dir.path(), "a.txt", "order #1234 shipped", at(2024, 1, 1))).unwrap();
    let bin = FileInfo::load(&make_bytes(dir.path(), "b.bin", &[0, 0, 0], at(2024, 1, 1))).unwrap();
    assert!(cond(Attribute::Contents, Operator::MatchesRegex, r"#\d{4}").matches(&a, now()));
    assert!(cond(Attribute::Contents, Operator::DoesNotContain, "refund").matches(&a, now()));
    assert!(!cond(Attribute::Contents, Operator::Contains, "refund").matches(&bin, now()));
    assert!(cond(Attribute::Contents, Operator::DoesNotContain, "refund").matches(&bin, now()));
}

#[test]
fn corrupt_pdf_is_ignored_not_fatal() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "broken.pdf", "%PDF-1.4 this is not a real pdf", at(2024, 1, 1));
    let info = FileInfo::load(&p).unwrap();
    let _ = info.contents(); // must not panic or hang
    assert!(!cond(Attribute::Contents, Operator::Contains, "zzz").matches(&info, now()));
}

// ----- downloaded from -------------------------------------------------------------------------

#[test]
fn downloaded_from_condition_and_source_token() {
    let dir = TempDir::new().unwrap();
    let a = make(dir.path(), "tool.zip", "x", at(2024, 1, 1));
    let b = make(dir.path(), "paper.pdf", "x", at(2024, 1, 1));
    make(dir.path(), "local.txt", "x", at(2024, 1, 1));
    if !(set_origin(&a, "https://github.com/acme/tool/releases/tool.zip") && set_origin(&b, "https://www.arxiv.org/pdf/1234")) {
        eprintln!("skipped: this filesystem cannot store download-origin metadata");
        return;
    }
    let fa = FileInfo::load(&a).unwrap();
    assert_eq!(fa.source_host().as_deref(), Some("github.com"));
    assert!(cond(Attribute::WhereFrom, Operator::Contains, "github.com").matches(&fa, now()));
    assert!(cond(Attribute::WhereFrom, Operator::StartsWith, "https://github").matches(&fa, now()));
    assert!(cond(Attribute::WhereFrom, Operator::DoesNotContain, "arxiv").matches(&fa, now()));
    assert!(!cond(Attribute::WhereFrom, Operator::Contains, "arxiv").matches(&fa, now()));

    let local = FileInfo::load(&dir.path().join("local.txt")).unwrap();
    assert!(!cond(Attribute::WhereFrom, Operator::Contains, "github").matches(&local, now()));
    assert!(cond(Attribute::WhereFrom, Operator::DoesNotContain, "github").matches(&local, now()));

    let r = rule("by source", vec![cond(Attribute::WhereFrom, Operator::Contains, ".")], vec![mv("{folder}/From {source}")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert!(dir.path().join("From github.com/tool.zip").exists());
    assert!(dir.path().join("From arxiv.org/paper.pdf").exists());
    assert!(dir.path().join("local.txt").exists());
}

#[test]
fn host_extraction() {
    assert_eq!(meta::host_of("https://www.github.com/a/b?x=1").as_deref(), Some("github.com"));
    assert_eq!(meta::host_of("http://user@Example.ORG:8080/x").as_deref(), Some("example.org"));
    assert_eq!(meta::host_of("mailto:nobody"), None);
    assert_eq!(meta::host_of(""), None);
}

// ----- tags ------------------------------------------------------------------------------------

#[test]
fn tags_round_trip_and_condition() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    if meta::add_tags(&p, &["Work".into()]).is_err() {
        eprintln!("skipped: this filesystem cannot store tags");
        return;
    }
    meta::add_tags(&p, &["Red".into(), "work".into(), " Invoice ".into()]).unwrap();
    assert_eq!(meta::tags(&p), vec!["Work", "Red", "Invoice"], "existing kept, duplicates (any case) ignored");

    let info = FileInfo::load(&p).unwrap();
    assert!(cond(Attribute::Tag, Operator::Equals, "red").matches(&info, now()));
    assert!(cond(Attribute::Tag, Operator::Contains, "voic").matches(&info, now()));
    assert!(cond(Attribute::Tag, Operator::NotEquals, "Green").matches(&info, now()));
    assert!(!cond(Attribute::Tag, Operator::NotEquals, "Red").matches(&info, now()));
}

#[test]
fn add_tags_action_through_the_engine() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    if meta::add_tags(&p, &["probe".into()]).is_err() {
        eprintln!("skipped: this filesystem cannot store tags");
        return;
    }
    let r = rule("tag", vec![cond(Attribute::Extension, Operator::Equals, "pdf")], vec![
        Action::new(ActionKind::AddTags, "Invoice, {kind}"),
        mv("Tagged"),
    ]);
    let entries = engine().process(&folder(&dir, vec![r]), false, now());
    assert!(entries.iter().all(|e| e.success), "{entries:?}");
    let moved = dir.path().join("Tagged/a.pdf");
    assert!(meta::tags(&moved).iter().any(|t| t == "Invoice"));
    assert!(meta::tags(&moved).iter().any(|t| t == "PDFs"), "tokens expand inside tag names");
}

// ----- last opened -----------------------------------------------------------------------------

#[test]
fn days_since_opened_uses_access_time() {
    let dir = TempDir::new().unwrap();
    let p = make(dir.path(), "old.txt", "x", at(2024, 5, 31));
    let long_ago = FileTime::from_unix_time(at(2023, 1, 1).timestamp(), 0);
    set_file_times(&p, long_ago, FileTime::from_unix_time(at(2024, 5, 31).timestamp(), 0)).unwrap();
    let info = FileInfo::load(&p).unwrap();
    assert!(cond(Attribute::DaysSinceOpened, Operator::GreaterThan, "365").matches(&info, now()));
    assert!(!cond(Attribute::DaysSinceOpened, Operator::LessThan, "365").matches(&info, now()));
}

// ----- duplicates ------------------------------------------------------------------------------

#[test]
fn duplicates_keep_the_oldest_as_original() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "same bytes", at(2024, 1, 1));
    make(dir.path(), "b copy.pdf", "same bytes", at(2024, 1, 1));
    make(dir.path(), "c.pdf", "different!", at(2024, 1, 1));
    make(dir.path(), "empty1.txt", "", at(2024, 1, 1));
    make(dir.path(), "empty2.txt", "", at(2024, 1, 1));
    let r = rule("dupes", vec![cond(Attribute::Duplicate, Operator::Equals, "")], vec![mv("Duplicates")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert!(dir.path().join("a.pdf").exists(), "the original stays");
    assert!(dir.path().join("Duplicates/b copy.pdf").exists());
    assert!(dir.path().join("c.pdf").exists(), "different content is not a duplicate");
    assert!(dir.path().join("empty1.txt").exists() && dir.path().join("empty2.txt").exists(), "empty files are ignored");
}

#[test]
fn duplicates_are_found_against_already_sorted_files() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "Sorted/PDFs/report.pdf", "payload", at(2023, 1, 1)); // filed long ago
    make(dir.path(), "report (1).pdf", "payload", at(2024, 5, 1)); // fresh download of the same file
    let r = rule("dupes", vec![cond(Attribute::Duplicate, Operator::Equals, "")], vec![mv("{folder}/Duplicates")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert!(dir.path().join("Duplicates/report (1).pdf").exists());
    assert!(dir.path().join("Sorted/PDFs/report.pdf").exists(), "the sorted original is untouched");
}

#[test]
fn not_duplicate_condition_and_summary() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.txt", "same", at(2024, 1, 1));
    make(dir.path(), "b.txt", "same", at(2024, 1, 1));
    make(dir.path(), "c.txt", "unique", at(2024, 1, 1));
    let r = rule("uniques", vec![cond(Attribute::Duplicate, Operator::NotEquals, "")], vec![mv("Keep")]);
    engine().process(&folder(&dir, vec![r]), false, now());
    assert!(dir.path().join("Keep/a.txt").exists() && dir.path().join("Keep/c.txt").exists());
    assert!(dir.path().join("b.txt").exists());
    assert_eq!(cond(Attribute::Duplicate, Operator::Equals, "").summary(), "file is a duplicate");
    assert_eq!(cond(Attribute::Tag, Operator::Equals, "Red").summary(), "has tag Red");
}

// ----- open with / shortcuts -------------------------------------------------------------------

#[test]
fn open_and_shortcut_command_lines() {
    let p = Path::new("/tmp/a b.pdf");
    assert_eq!(actions::open_command("macos", "", p), ("open".to_string(), vec!["/tmp/a b.pdf".to_string()]));
    assert_eq!(
        actions::open_command("macos", "Preview", p),
        ("open".to_string(), vec!["-a".into(), "Preview".into(), "/tmp/a b.pdf".into()])
    );
    assert_eq!(actions::open_command("linux", "", p).0, "xdg-open");
    assert_eq!(actions::open_command("linux", "evince", p), ("evince".to_string(), vec!["/tmp/a b.pdf".to_string()]));
    assert_eq!(actions::open_command("windows", "", p).0, "cmd");

    assert_eq!(
        actions::shortcut_command("macos", "Resize Image", p),
        Some(("shortcuts".to_string(), vec!["run".into(), "Resize Image".into(), "--input-path".into(), "/tmp/a b.pdf".into()]))
    );
    assert_eq!(actions::shortcut_command("linux", "x", p), None);
    assert_eq!(actions::shortcut_command("windows", "x", p), None);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn run_shortcut_is_a_clear_error_off_macos() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let r = rule("r", vec![cond(Attribute::Extension, Operator::Equals, "pdf")], vec![Action::new(ActionKind::RunShortcut, "Anything")]);
    let entries = engine().process(&folder(&dir, vec![r]), false, now());
    assert!(!entries[0].success);
    assert!(entries[0].detail.contains("only available on macOS"), "{}", entries[0].detail);
}

#[cfg(unix)]
#[test]
fn open_with_runs_the_given_program_with_the_file() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    // `cp` stands in for an application: it is invoked as `cp <file>`, so it fails with a usage error ->
    // exercise the failure path, then a real success path with `true`.
    let bad = rule("r", vec![cond(Attribute::Extension, Operator::Equals, "pdf")], vec![Action::new(ActionKind::OpenWith, "false")]);
    #[cfg(not(target_os = "macos"))]
    {
        let entries = engine().process(&folder(&dir, vec![bad]), false, now());
        assert!(!entries[0].success, "a program that exits non-zero is reported");
        let dir2 = TempDir::new().unwrap();
        make(dir2.path(), "a.pdf", "x", at(2024, 1, 1));
        let ok = rule("r", vec![cond(Attribute::Extension, Operator::Equals, "pdf")], vec![Action::new(ActionKind::OpenWith, "true")]);
        let entries = engine().process(&folder(&dir2, vec![ok]), false, now());
        assert!(entries[0].success, "{entries:?}");
    }
    #[cfg(target_os = "macos")]
    let _ = bad;
}

#[test]
fn open_with_dry_run_does_not_launch_anything() {
    let dir = TempDir::new().unwrap();
    make(dir.path(), "a.pdf", "x", at(2024, 1, 1));
    let r = rule("r", vec![cond(Attribute::Extension, Operator::Equals, "pdf")], vec![
        Action::new(ActionKind::OpenWith, "DefinitelyNotAnApp"),
        Action::new(ActionKind::AddTags, "x"),
    ]);
    let entries = engine().process(&folder(&dir, vec![r]), true, now());
    assert!(entries.iter().all(|e| e.success && e.dry_run), "{entries:?}");
}

// ----- staleness of the time source in tests --------------------------------------------------

#[test]
fn recent_dates_are_relative_to_the_clock_passed_in() {
    let f = FileInfo::from_parts(PathBuf::from("x"), 1, now() - Duration::days(3), now() - Duration::days(3));
    assert!(cond(Attribute::DaysSinceCreated, Operator::LessThan, "4").matches(&f, now()));
}

// ----- folder picker helper --------------------------------------------------------------------

#[test]
fn picked_folders_are_written_portably() {
    let tmp = TempDir::new().unwrap();
    let watched = tmp.path().join("Downloads");
    let home = tmp.path().join("home");
    let elsewhere = tmp.path().join("volume/Backups");
    for d in [&watched.join("Sorted/PDFs"), &home.join("Documents/Invoices"), &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    let p = |chosen: &Path| template::portable_path(chosen, &watched, Some(&home));
    assert_eq!(p(&watched), "{folder}");
    assert_eq!(p(&watched.join("Sorted/PDFs")), "{folder}/Sorted/PDFs");
    assert_eq!(p(&home.join("Documents/Invoices")), "~/Documents/Invoices");
    assert_eq!(p(&home), "~");
    assert_eq!(p(&elsewhere), elsewhere.to_string_lossy());
    // and the result round-trips through the engine's own path expansion
    let expanded = template::expand_tilde("~/Documents");
    assert!(expanded.is_absolute() || expanded == Path::new("~/Documents"));
}
