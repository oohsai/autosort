use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::fileinfo::FileInfo;

pub const TOKEN_HELP: &str =
    "{name} {filename} {ext} {kind} {year} {month} {monthName} {day} {date} {today} {folder}";

/// Expands `{tokens}` in destination folders and new file names.
///
/// `{name}` base name, `{filename}` full name, `{ext}`, `{kind}`, `{year}` `{month}` `{monthName}` `{day}`
/// `{date}` (from the file's modification date), `{today}` (current date), `{folder}` (the watched folder).
pub fn expand(template: &str, info: &FileInfo, folder: &Path, now: DateTime<Local>) -> String {
    let m = info.modified;
    let pairs: [(&str, String); 11] = [
        ("{name}", info.base_name.clone()),
        ("{filename}", info.name.clone()),
        ("{ext}", info.ext.clone()),
        ("{kind}", info.kind().display_name().to_string()),
        ("{year}", m.format("%Y").to_string()),
        ("{month}", m.format("%m").to_string()),
        ("{monthName}", m.format("%B").to_string()),
        ("{day}", m.format("%d").to_string()),
        ("{date}", m.format("%Y-%m-%d").to_string()),
        ("{today}", now.format("%Y-%m-%d").to_string()),
        ("{folder}", folder.to_string_lossy().into_owned()),
    ];
    let mut out = template.to_string();
    for (token, value) in pairs {
        out = out.replace(token, &value);
    }
    out
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
