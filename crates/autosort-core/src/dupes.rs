use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

/// Finds files with identical content under a folder (including sorted subfolders), computed lazily and
/// only if some rule actually asks. Files are grouped by size first, so only plausible matches are hashed.
///
/// In each group of identical files the oldest one is the "original"; every other copy is a duplicate.
pub struct DupIndex {
    root: PathBuf,
    flags: OnceLock<HashMap<PathBuf, bool>>,
}

impl std::fmt::Debug for DupIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DupIndex({})", self.root.display())
    }
}

impl DupIndex {
    pub fn new(root: &Path) -> Self {
        DupIndex { root: root.to_path_buf(), flags: OnceLock::new() }
    }

    pub fn is_duplicate(&self, path: &Path) -> bool {
        self.flags.get_or_init(|| build(&self.root)).get(path).copied().unwrap_or(false)
    }
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64, SystemTime)>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_dir() {
            walk(&path, out);
        } else if ft.is_file() {
            if let Ok(md) = entry.metadata() {
                if md.len() > 0 {
                    let when = md.created().or_else(|_| md.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                    out.push((path, md.len(), when));
                }
            }
        }
    }
}

fn hash_file(path: &Path) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(*hasher.finalize().as_bytes())
}

fn build(root: &Path) -> HashMap<PathBuf, bool> {
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut by_size: HashMap<u64, Vec<(PathBuf, SystemTime)>> = HashMap::new();
    for (p, size, when) in files {
        by_size.entry(size).or_default().push((p, when));
    }
    let mut flags = HashMap::new();
    for group in by_size.into_values().filter(|g| g.len() > 1) {
        let mut by_hash: HashMap<[u8; 32], Vec<(PathBuf, SystemTime)>> = HashMap::new();
        for (p, when) in group {
            if let Some(h) = hash_file(&p) {
                by_hash.entry(h).or_default().push((p, when));
            }
        }
        for mut same in by_hash.into_values().filter(|g| g.len() > 1) {
            same.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            for (i, (p, _)) in same.into_iter().enumerate() {
                flags.insert(p, i > 0);
            }
        }
    }
    flags
}
