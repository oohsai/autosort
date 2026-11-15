use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::kind::FileKind;

/// Snapshot of the attributes rules can match on.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,      // "report.pdf"
    pub base_name: String, // "report"
    pub ext: String,       // "pdf" (lowercased)
    pub size: u64,
    pub created: DateTime<Local>,
    pub modified: DateTime<Local>,
}

impl FileInfo {
    pub fn from_parts(path: PathBuf, size: u64, created: DateTime<Local>, modified: DateTime<Local>) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let base_name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        FileInfo { path, name, base_name, ext, size, created, modified }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let md = std::fs::metadata(path)?;
        let modified: DateTime<Local> = md.modified()?.into();
        // Creation time is not available on every filesystem; fall back to modified.
        let created: DateTime<Local> = md.created().map(Into::into).unwrap_or(modified);
        Ok(Self::from_parts(path.to_path_buf(), md.len(), created, modified))
    }

    pub fn kind(&self) -> FileKind {
        FileKind::classify(&self.ext)
    }

    /// The same file after a move/rename (size and dates carry over).
    pub fn relocated(&self, new_path: PathBuf) -> FileInfo {
        FileInfo::from_parts(new_path, self.size, self.created, self.modified)
    }
}
