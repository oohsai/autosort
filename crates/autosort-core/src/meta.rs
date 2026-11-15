//! Lazy, best-effort readers for file metadata that is not in a plain `stat`:
//! where a file was downloaded from, a photo's date taken, Finder tags, and searchable text contents.
//!
//! Every function returns "nothing" rather than failing: a file without that metadata simply doesn't match.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};

// ------------------------------------------------------------------ download origin

/// URLs a file was downloaded from (most specific first).
///
/// macOS: `com.apple.metadata:kMDItemWhereFroms`. Linux: `user.xdg.origin.url` / `user.xdg.referrer.url`.
/// Windows: the `Zone.Identifier` stream (`HostUrl`, `ReferrerUrl`).
pub fn origin_urls(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Ok(Some(bytes)) = xattr::get(path, "com.apple.metadata:kMDItemWhereFroms") {
            if let Ok(plist::Value::Array(items)) = plist::Value::from_reader(std::io::Cursor::new(bytes)) {
                out.extend(items.into_iter().filter_map(|v| v.into_string()));
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for attr in ["user.xdg.origin.url", "user.xdg.referrer.url"] {
            if let Ok(Some(bytes)) = xattr::get(path, attr) {
                out.push(String::from_utf8_lossy(&bytes).trim().to_string());
            }
        }
    }
    #[cfg(windows)]
    {
        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        if let Ok(text) = fs::read_to_string(std::path::PathBuf::from(stream)) {
            for line in text.lines() {
                if let Some(u) = line.strip_prefix("HostUrl=").or_else(|| line.strip_prefix("ReferrerUrl=")) {
                    out.push(u.trim().to_string());
                }
            }
        }
    }
    let _ = path; // silences "unused" on platforms without any reader
    out.retain(|u| !u.is_empty());
    out
}

/// "https://www.github.com/a/b?x" -> "github.com".
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    let host = host.trim().trim_start_matches("www.").to_lowercase();
    (!host.is_empty() && host.contains('.')).then_some(host)
}

// ----------------------------------------------------------------------- EXIF date

/// Date a photo was taken (EXIF `DateTimeOriginal`, then `DateTimeDigitized`, then `DateTime`).
pub fn exif_taken(path: &Path) -> Option<DateTime<Local>> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if !["jpg", "jpeg", "tif", "tiff", "heic", "heif", "png", "webp"].contains(&ext.as_str()) {
        return None;
    }
    let file = File::open(path).ok()?;
    let exif = exif::Reader::new().read_from_container(&mut BufReader::new(file)).ok()?;
    for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTimeDigitized, exif::Tag::DateTime] {
        let Some(field) = exif.get_field(tag, exif::In::PRIMARY) else { continue };
        let text = field.display_value().to_string();
        // kamadak-exif prints "2023-07-14 09:30:00"; raw ASCII uses colons. Accept both.
        let naive = NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(text.trim(), "%Y:%m:%d %H:%M:%S"));
        if let Some(dt) = naive.ok().and_then(|n| Local.from_local_datetime(&n).single()) {
            return Some(dt);
        }
    }
    None
}

// --------------------------------------------------------------------------- tags

/// Finder colour labels are tags whose stored name carries a colour number.
#[cfg(target_os = "macos")]
const COLORS: [(&str, u8); 7] =
    [("gray", 1), ("grey", 1), ("green", 2), ("purple", 3), ("blue", 4), ("yellow", 5), ("red", 6)];
#[cfg(target_os = "macos")]
const ORANGE: u8 = 7;

#[cfg(target_os = "macos")]
fn color_number(name: &str) -> Option<u8> {
    let n = name.trim().to_lowercase();
    if n == "orange" {
        return Some(ORANGE);
    }
    COLORS.iter().find(|(c, _)| *c == n).map(|(_, v)| *v)
}

/// Tag names on a file (macOS Finder tags; `user.xdg.tags` elsewhere on unix). Colour suffixes are stripped.
pub fn tags(path: &Path) -> Vec<String> {
    stored_tags(path).into_iter().map(|t| t.split('\n').next().unwrap_or("").to_string()).collect()
}

/// Raw stored entries, e.g. "Red\n6".
fn stored_tags(path: &Path) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(Some(bytes)) = xattr::get(path, "com.apple.metadata:_kMDItemUserTags") {
            if let Ok(plist::Value::Array(items)) = plist::Value::from_reader(std::io::Cursor::new(bytes)) {
                return items.into_iter().filter_map(|v| v.into_string()).collect();
            }
        }
        Vec::new()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        match xattr::get(path, "user.xdg.tags") {
            Ok(Some(bytes)) => String::from_utf8_lossy(&bytes)
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Vec::new()
    }
}

/// Adds tags (existing ones are kept, duplicates ignored). Colour names ("Red", "Green"…) become
/// Finder colour labels on macOS.
pub fn add_tags(path: &Path, names: &[String]) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut entries = stored_tags(path);
        for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
            if entries.iter().any(|e| e.split('\n').next().unwrap_or("").eq_ignore_ascii_case(name)) {
                continue;
            }
            #[cfg(target_os = "macos")]
            entries.push(match color_number(name) {
                Some(n) => format!("{name}\n{n}"),
                None => name.to_string(),
            });
            #[cfg(not(target_os = "macos"))]
            entries.push(name.to_string());
        }
        write_tags(path, &entries)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, names);
        Err("tags are not supported on this system".into())
    }
}

#[cfg(target_os = "macos")]
fn write_tags(path: &Path, entries: &[String]) -> Result<(), String> {
    let value = plist::Value::Array(entries.iter().map(|e| plist::Value::String(e.clone())).collect());
    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &value).map_err(|e| format!("could not encode tags: {e}"))?;
    xattr::set(path, "com.apple.metadata:_kMDItemUserTags", &buf).map_err(|e| format!("could not write tags: {e}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn write_tags(path: &Path, entries: &[String]) -> Result<(), String> {
    xattr::set(path, "user.xdg.tags", entries.join(",").as_bytes()).map_err(|e| format!("could not write tags: {e}"))
}

// ----------------------------------------------------------------------- contents

const MAX_READ: u64 = 4 * 1024 * 1024;
const MAX_PDF: u64 = 60 * 1024 * 1024;
const MAX_CHARS: usize = 1_000_000;

/// Searchable text of a file: plain text, PDF, and Office/OpenDocument files. `None` if unreadable or binary.
pub fn text_contents(path: &Path) -> Option<String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let size = fs::metadata(path).ok()?.len();
    let text = match ext.as_str() {
        "pdf" if size <= MAX_PDF => pdf_text(path)?,
        "docx" | "pptx" | "xlsx" | "odt" | "ods" | "odp" => office_text(path, &ext)?,
        _ => plain_text(path)?,
    };
    Some(text.chars().take(MAX_CHARS).collect())
}

fn plain_text(path: &Path) -> Option<String> {
    let mut buf = Vec::new();
    File::open(path).ok()?.take(MAX_READ).read_to_end(&mut buf).ok()?;
    if buf.iter().take(8192).any(|b| *b == 0) {
        return None; // binary
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn pdf_text(path: &Path) -> Option<String> {
    // The PDF parser can panic on malformed files; a bad PDF must never take the app down.
    let path = path.to_path_buf();
    std::panic::catch_unwind(move || pdf_extract::extract_text(&path).ok()).ok().flatten()
}

fn office_text(path: &Path, ext: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path).ok()?)).ok()?;
    let wanted: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .filter(|n| match ext {
            "docx" => n == "word/document.xml",
            "pptx" => n.starts_with("ppt/slides/slide") && n.ends_with(".xml"),
            "xlsx" => n == "xl/sharedStrings.xml",
            _ => n == "content.xml",
        })
        .collect();
    let tag = regex::Regex::new(r"<[^>]*>").ok()?;
    let mut out = String::new();
    for name in wanted {
        let mut xml = String::new();
        zip.by_name(&name).ok()?.take(MAX_READ).read_to_string(&mut xml).ok()?;
        out.push_str(&tag.replace_all(&xml, " "));
        out.push(' ');
    }
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}
