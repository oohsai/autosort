use serde::{Deserialize, Serialize};

/// Coarse file categories derived from the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Image,
    Video,
    Audio,
    Pdf,
    Document,
    Spreadsheet,
    Presentation,
    Archive,
    Code,
    Installer,
    Other,
}

impl FileKind {
    pub const ALL: [FileKind; 11] = [
        FileKind::Image,
        FileKind::Video,
        FileKind::Audio,
        FileKind::Pdf,
        FileKind::Document,
        FileKind::Spreadsheet,
        FileKind::Presentation,
        FileKind::Archive,
        FileKind::Code,
        FileKind::Installer,
        FileKind::Other,
    ];

    /// Stable machine key, also what a `Kind` condition stores as its value.
    pub fn key(self) -> &'static str {
        match self {
            FileKind::Image => "image",
            FileKind::Video => "video",
            FileKind::Audio => "audio",
            FileKind::Pdf => "pdf",
            FileKind::Document => "document",
            FileKind::Spreadsheet => "spreadsheet",
            FileKind::Presentation => "presentation",
            FileKind::Archive => "archive",
            FileKind::Code => "code",
            FileKind::Installer => "installer",
            FileKind::Other => "other",
        }
    }

    /// Folder-friendly name used by the `{kind}` template token, e.g. "Images".
    pub fn display_name(self) -> &'static str {
        match self {
            FileKind::Image => "Images",
            FileKind::Video => "Videos",
            FileKind::Audio => "Audio",
            FileKind::Pdf => "PDFs",
            FileKind::Document => "Documents",
            FileKind::Spreadsheet => "Spreadsheets",
            FileKind::Presentation => "Presentations",
            FileKind::Archive => "Archives",
            FileKind::Code => "Code",
            FileKind::Installer => "Installers",
            FileKind::Other => "Other",
        }
    }

    pub fn from_key(key: &str) -> Option<FileKind> {
        FileKind::ALL.into_iter().find(|k| k.key() == key.trim().to_lowercase())
    }

    pub fn classify(ext: &str) -> FileKind {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "webp" | "tif" | "tiff" | "bmp"
            | "svg" | "raw" | "cr2" | "nef" | "arw" | "dng" | "psd" | "ai" | "ico" => FileKind::Image,
            "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "mpg" | "mpeg" => FileKind::Video,
            "mp3" | "wav" | "aac" | "m4a" | "flac" | "ogg" | "aiff" | "aif" | "wma" => FileKind::Audio,
            "pdf" => FileKind::Pdf,
            "doc" | "docx" | "pages" | "txt" | "rtf" | "md" | "odt" | "epub" | "mobi" | "tex" => FileKind::Document,
            "xls" | "xlsx" | "numbers" | "csv" | "tsv" | "ods" => FileKind::Spreadsheet,
            "ppt" | "pptx" | "key" | "odp" => FileKind::Presentation,
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "iso" => FileKind::Archive,
            "rs" | "swift" | "py" | "js" | "ts" | "tsx" | "jsx" | "json" | "html" | "css" | "c" | "h"
            | "cpp" | "m" | "mm" | "java" | "kt" | "go" | "rb" | "php" | "sh" | "yml" | "yaml" | "xml"
            | "sql" | "ipynb" | "toml" => FileKind::Code,
            "dmg" | "pkg" | "mpkg" | "exe" | "msi" | "deb" | "rpm" | "appimage" => FileKind::Installer,
            _ => FileKind::Other,
        }
    }
}
