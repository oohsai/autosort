//! "Sort these files now": used by `autosort --sort` and the file-manager (Finder) integration.

use std::path::{Path, PathBuf};

use crate::engine::Engine;
use crate::model::{AppConfig, WatchedFolder};

/// Files to run through one watched folder's rules.
#[derive(Debug, Clone)]
pub struct SortPlan {
    pub folder: WatchedFolder,
    pub files: Vec<PathBuf>,
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Maps the given paths to the watched folders that contain them.
///
/// A file is sorted with the rules of the (innermost) watched folder it lives in. A folder is expanded to
/// the files inside it. Paths outside every watched folder are returned with an explanation.
pub fn plan_sort(cfg: &AppConfig, paths: &[PathBuf]) -> (Vec<SortPlan>, Vec<(PathBuf, String)>) {
    let mut plans: Vec<SortPlan> = Vec::new();
    let mut rejected = Vec::new();
    let roots: Vec<(PathBuf, &WatchedFolder)> = cfg.folders.iter().map(|f| (canon(&f.root()), f)).collect();

    for original in paths {
        let path = canon(original);
        if !path.exists() {
            rejected.push((original.clone(), "does not exist".to_string()));
            continue;
        }
        // innermost watched folder containing the path
        let Some((root, folder)) = roots.iter().filter(|(r, _)| path.starts_with(r)).max_by_key(|(r, _)| r.components().count()) else {
            rejected.push((original.clone(), "is not inside a watched folder — add its folder in AutoSort first".to_string()));
            continue;
        };
        let files: Vec<PathBuf> = if path.is_dir() {
            // the watched root honours "include subfolders"; a subfolder you picked explicitly is searched deeply
            let deep = folder.include_subfolders || path != *root;
            Engine::candidate_files_ignoring(root, true, &folder.ignore)
                .into_iter()
                .filter(|f| f.starts_with(&path))
                .filter(|f| deep || f.parent() == Some(root.as_path()))
                .collect()
        } else {
            vec![path.clone()]
        };
        match plans.iter_mut().find(|p| p.folder.id == folder.id) {
            Some(plan) => plan.files.extend(files),
            None => plans.push(SortPlan { folder: (*folder).clone(), files }),
        }
    }
    for p in &mut plans {
        p.files.sort();
        p.files.dedup();
    }
    (plans, rejected)
}
