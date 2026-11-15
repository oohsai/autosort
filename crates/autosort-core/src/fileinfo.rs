use std::cell::OnceCell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Local};

use crate::dupes::DupIndex;
use crate::kind::FileKind;
use crate::meta;

/// Snapshot of the attributes rules can match on. Expensive metadata (origin URL, EXIF, contents, tags)
/// is read lazily, only when a rule or template asks for it, and cached.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,      // "report.pdf"
    pub base_name: String, // "report"
    pub ext: String,       // "pdf" (lowercased)
    pub size: u64,
    pub created: DateTime<Local>,
    pub modified: DateTime<Local>,
    pub accessed: DateTime<Local>,
    origin: OnceCell<Vec<String>>,
    taken: OnceCell<Option<DateTime<Local>>>,
    contents: OnceCell<Option<String>>,
    tags: OnceCell<Vec<String>>,
    dup: Option<Arc<DupIndex>>,
    dup_key: PathBuf,
}

impl FileInfo {
    pub fn from_parts(path: PathBuf, size: u64, created: DateTime<Local>, modified: DateTime<Local>) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let base_name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        FileInfo {
            dup_key: path.clone(),
            path,
            name,
            base_name,
            ext,
            size,
            created,
            modified,
            accessed: modified,
            origin: OnceCell::new(),
            taken: OnceCell::new(),
            contents: OnceCell::new(),
            tags: OnceCell::new(),
            dup: None,
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let md = std::fs::metadata(path)?;
        let modified: DateTime<Local> = md.modified()?.into();
        // Creation time is not available on every filesystem; fall back to modified.
        let created: DateTime<Local> = md.created().map(Into::into).unwrap_or(modified);
        let mut info = Self::from_parts(path.to_path_buf(), md.len(), created, modified);
        if let Ok(a) = md.accessed() {
            info.accessed = a.into();
        }
        Ok(info)
    }

    /// Lets duplicate checks see the whole watched folder.
    pub fn with_dup_index(mut self, index: Arc<DupIndex>) -> Self {
        self.dup = Some(index);
        self
    }

    pub fn kind(&self) -> FileKind {
        FileKind::classify(&self.ext)
    }

    /// URLs the file was downloaded from, if the OS recorded any.
    pub fn origin_urls(&self) -> &[String] {
        self.origin.get_or_init(|| meta::origin_urls(&self.path))
    }

    /// Website the file came from, e.g. "github.com".
    pub fn source_host(&self) -> Option<String> {
        self.origin_urls().iter().find_map(|u| meta::host_of(u))
    }

    /// EXIF date taken (photos), if present.
    pub fn taken(&self) -> Option<DateTime<Local>> {
        *self.taken.get_or_init(|| meta::exif_taken(&self.path))
    }

    /// Searchable text contents (text, PDF, Office).
    pub fn contents(&self) -> Option<&str> {
        self.contents.get_or_init(|| meta::text_contents(&self.path)).as_deref()
    }

    pub fn tags(&self) -> &[String] {
        self.tags.get_or_init(|| meta::tags(&self.path))
    }

    pub fn is_duplicate(&self) -> bool {
        self.dup.as_ref().is_some_and(|d| d.is_duplicate(&self.dup_key))
    }

    /// The same file after a move/rename (size, dates and cached metadata carry over; tags are re-read
    /// because actions may have changed them).
    pub fn relocated(&self, new_path: PathBuf) -> FileInfo {
        let mut next = FileInfo::from_parts(new_path, self.size, self.created, self.modified);
        next.accessed = self.accessed;
        next.origin = self.origin.clone();
        next.taken = self.taken.clone();
        next.contents = self.contents.clone();
        next.dup = self.dup.clone();
        next.dup_key = self.dup_key.clone();
        next
    }
}
