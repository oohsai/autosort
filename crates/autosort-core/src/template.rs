use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use regex::Regex;

use crate::fileinfo::FileInfo;

pub const TOKEN_HELP: &str = "{name} {filename} {ext} {kind} {year} {month} {monthName} {day} {date} {today} {folder} \
{source} {takenYear} {takenMonth} {takenDay} {takenDate} {counter:3}  ·  filters: {name|lower|kebab|replace:a:b|slice:0:20}";

/// Expands `{tokens}` (optionally piped through filters) in destination folders and new file names.
///
/// Tokens: `{name}` base name, `{filename}`, `{ext}`, `{kind}`, `{year}` `{month}` `{monthName}` `{day}` `{date}`
/// (file's modification date), `{today}`, `{folder}` (the watched folder), `{source}` (website it was downloaded
/// from), `{takenYear}` `{takenMonth}` `{takenMonthName}` `{takenDay}` `{takenDate}` (photo date, falling back
/// to the modification date), and `{counter}` / `{counter:3}` (next free number; only meaningful when renaming).
///
/// Filters, chained with `|`: `upper`, `lower`, `title`, `snake`, `kebab`, `trim`, `replace:old:new`,
/// `regex:pattern:replacement` (no `|` or `:` inside the pattern), `slice:start:end` (characters).
/// Unknown tokens and filters are left as written so typos are visible.
pub fn expand(template: &str, info: &FileInfo, folder: &Path, now: DateTime<Local>) -> String {
    expand_with(template, info, folder, now, None)
}

pub fn expand_with(template: &str, info: &FileInfo, folder: &Path, now: DateTime<Local>, counter: Option<u32>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &after[..end];
        match resolve(inner, info, folder, now, counter) {
            Some(v) => out.push_str(&v),
            None => {
                out.push('{');
                out.push_str(inner);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// True if the template asks for a `{counter}`.
pub fn uses_counter(template: &str) -> bool {
    template.contains("{counter")
}

fn resolve(inner: &str, info: &FileInfo, folder: &Path, now: DateTime<Local>, counter: Option<u32>) -> Option<String> {
    let mut parts = inner.split('|');
    let token = parts.next()?.trim();
    let mut value = base_value(token, info, folder, now, counter)?;
    for filter in parts {
        value = apply_filter(filter, &value)?;
    }
    Some(value)
}

fn base_value(token: &str, info: &FileInfo, folder: &Path, now: DateTime<Local>, counter: Option<u32>) -> Option<String> {
    let m = info.modified;
    let taken = info.taken().unwrap_or(info.modified);
    if let Some(width) = token.strip_prefix("counter") {
        let width: usize = match width.strip_prefix(':') {
            Some(w) => w.trim().parse().ok()?,
            None if width.is_empty() => 1,
            None => return None,
        };
        return Some(format!("{:0width$}", counter.unwrap_or(1)));
    }
    Some(match token {
        "name" => info.base_name.clone(),
        "filename" => info.name.clone(),
        "ext" => info.ext.clone(),
        "kind" => info.kind().display_name().to_string(),
        "year" => m.format("%Y").to_string(),
        "month" => m.format("%m").to_string(),
        "monthName" => m.format("%B").to_string(),
        "day" => m.format("%d").to_string(),
        "date" => m.format("%Y-%m-%d").to_string(),
        "today" => now.format("%Y-%m-%d").to_string(),
        "folder" => folder.to_string_lossy().into_owned(),
        "source" => info.source_host().unwrap_or_else(|| "Unknown".to_string()),
        "takenYear" => taken.format("%Y").to_string(),
        "takenMonth" => taken.format("%m").to_string(),
        "takenMonthName" => taken.format("%B").to_string(),
        "takenDay" => taken.format("%d").to_string(),
        "takenDate" => taken.format("%Y-%m-%d").to_string(),
        _ => return None,
    })
}

fn words(s: &str) -> Vec<String> {
    // split on non-alphanumerics and on lower->Upper camel boundaries
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn apply_filter(filter: &str, v: &str) -> Option<String> {
    let mut args = filter.split(':');
    let name = args.next()?.trim();
    let rest: Vec<&str> = args.collect();
    Some(match name {
        "upper" => v.to_uppercase(),
        "lower" => v.to_lowercase(),
        "trim" => v.trim().to_string(),
        "title" => v
            .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
            .map(|w| {
                let mut cs = w.chars();
                match cs.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        "snake" => words(v).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_"),
        "kebab" => words(v).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("-"),
        "replace" => {
            let [old, new] = rest[..] else { return None };
            if old.is_empty() { v.to_string() } else { v.replace(old, new) }
        }
        "regex" => {
            let [pat, repl] = rest[..] else { return None };
            Regex::new(pat).ok()?.replace_all(v, repl).into_owned()
        }
        "slice" => {
            let chars: Vec<char> = v.chars().collect();
            let start: usize = rest.first()?.trim().parse().ok()?;
            let end: usize = match rest.get(1) {
                Some(e) if !e.trim().is_empty() => e.trim().parse().ok()?,
                _ => chars.len(),
            };
            chars.get(start.min(chars.len())..end.min(chars.len()).max(start.min(chars.len())))?.iter().collect()
        }
        _ => return None,
    })
}

/// Expands a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    let home = || directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf());
    if path == "~" {
        return home().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(h) = home() {
            return h.join(rest);
        }
    }
    PathBuf::from(path)
}
