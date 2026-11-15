use std::fs;
use std::path::{Path, PathBuf};

use autosort_core::*;
use tempfile::TempDir;

fn cfg_with(dirs: &[(&Path, bool)]) -> AppConfig {
    let mut c = AppConfig::default();
    for (d, sub) in dirs {
        let mut f = WatchedFolder::new(d.to_string_lossy());
        f.include_subfolders = *sub;
        c.folders.push(f);
    }
    c
}
fn touch(p: &Path) -> PathBuf {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, "x").unwrap();
    p.to_path_buf()
}

// ---------------------------------------------------------------------------- plan_sort

#[test]
fn files_map_to_their_watched_folder_and_outsiders_are_rejected() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let fa = touch(&a.path().join("x.pdf"));
    let fb = touch(&b.path().join("y.pdf"));
    let fo = touch(&outside.path().join("z.pdf"));
    let cfg = cfg_with(&[(a.path(), false), (b.path(), false)]);
    let missing = a.path().join("nope.pdf");

    let (plans, rejected) = sorting::plan_sort(&cfg, &[fa.clone(), fb.clone(), fo.clone(), missing.clone()]);
    assert_eq!(plans.len(), 2);
    let canon = |p: &Path| fs::canonicalize(p).unwrap();
    assert_eq!(plans.iter().find(|p| p.folder.path == cfg.folders[0].path).unwrap().files, vec![canon(&fa)]);
    assert_eq!(plans.iter().find(|p| p.folder.path == cfg.folders[1].path).unwrap().files, vec![canon(&fb)]);
    assert_eq!(rejected.len(), 2);
    assert!(rejected.iter().any(|(p, why)| *p == fo && why.contains("not inside a watched folder")));
    assert!(rejected.iter().any(|(p, why)| *p == missing && why.contains("does not exist")));
}

#[test]
fn the_innermost_watched_folder_wins() {
    let outer = TempDir::new().unwrap();
    let inner = outer.path().join("Inner");
    let f = touch(&inner.join("a.pdf"));
    let cfg = cfg_with(&[(outer.path(), true), (&inner, false)]);
    let (plans, _) = sorting::plan_sort(&cfg, &[f]);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].folder.path, inner.to_string_lossy());
}

#[test]
fn folders_expand_to_files_and_respect_include_subfolders_and_ignore() {
    let root = TempDir::new().unwrap();
    touch(&root.path().join("top.pdf"));
    touch(&root.path().join("sub/deep.pdf"));
    touch(&root.path().join("skip.psd"));
    touch(&root.path().join(".hidden"));

    let mut cfg = cfg_with(&[(root.path(), false)]);
    cfg.folders[0].ignore = vec!["*.psd".into()];
    let names = |plans: &[sorting::SortPlan]| -> Vec<String> {
        plans.iter().flat_map(|p| p.files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned())).collect()
    };
    // the watched root itself follows "include subfolders" (off)
    let (plans, _) = sorting::plan_sort(&cfg, &[root.path().to_path_buf()]);
    assert_eq!(names(&plans), vec!["top.pdf"]);
    // ...but a subfolder picked explicitly is searched
    let (plans, _) = sorting::plan_sort(&cfg, &[root.path().join("sub")]);
    assert_eq!(names(&plans), vec!["deep.pdf"]);
    // with "include subfolders" on, the root includes everything not ignored
    cfg.folders[0].include_subfolders = true;
    let (plans, _) = sorting::plan_sort(&cfg, &[root.path().to_path_buf()]);
    assert_eq!(names(&plans), vec!["deep.pdf", "top.pdf"]);
}

#[test]
fn the_same_file_given_twice_is_listed_once() {
    let root = TempDir::new().unwrap();
    let f = touch(&root.path().join("a.pdf"));
    let cfg = cfg_with(&[(root.path(), false)]);
    let (plans, _) = sorting::plan_sort(&cfg, &[f.clone(), f, root.path().to_path_buf()]);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].files.len(), 1);
}

// ------------------------------------------------------------------------- integration

#[test]
fn macos_workflow_is_valid_plist_and_runs_our_exe_quoted() {
    let exe = "/Applications/Auto Sort's/AutoSort.app/Contents/MacOS/autosort";
    let files = integration::macos_workflow_files(exe);
    assert_eq!(files.len(), 2);
    for (name, text) in &files {
        let v = plist::Value::from_reader_xml(text.as_bytes()).unwrap_or_else(|e| panic!("{name} is not a valid plist: {e}"));
        assert!(v.as_dictionary().is_some(), "{name}");
    }
    let info = plist::Value::from_reader_xml(files[0].1.as_bytes()).unwrap();
    let services = info.as_dictionary().unwrap().get("NSServices").unwrap().as_array().unwrap();
    let menu = services[0].as_dictionary().unwrap().get("NSMenuItem").unwrap().as_dictionary().unwrap();
    assert_eq!(menu.get("default").unwrap().as_string(), Some("Sort with AutoSort"));

    let wf = plist::Value::from_reader_xml(files[1].1.as_bytes()).unwrap();
    let action = wf.as_dictionary().unwrap().get("actions").unwrap().as_array().unwrap()[0]
        .as_dictionary().unwrap().get("action").unwrap().as_dictionary().unwrap()
        .get("ActionParameters").unwrap().as_dictionary().unwrap();
    let cmd = action.get("COMMAND_STRING").unwrap().as_string().unwrap();
    assert_eq!(cmd, "'/Applications/Auto Sort'\\''s/AutoSort.app/Contents/MacOS/autosort' --sort \"$@\"");
    assert_eq!(action.get("inputMethod").unwrap().as_signed_integer(), Some(1), "files arrive as arguments");
}

#[test]
fn install_and_uninstall_files_for_macos_and_linux() {
    for os in ["macos", "linux"] {
        let home = TempDir::new().unwrap();
        assert!(!integration::is_installed_in(home.path(), os));
        let exe = Path::new("/opt/autosort/autosort");
        if os == "macos" {
            // `pbs -update` only exists on a real Mac; installing elsewhere just writes the files
            integration::install_in(home.path(), os, exe).unwrap();
            assert!(home.path().join("Library/Services/Sort with AutoSort.workflow/Contents/document.wflow").exists());
        } else {
            integration::install_in(home.path(), os, exe).unwrap();
            assert!(home.path().join(".local/share/nautilus/scripts/Sort with AutoSort").exists());
            assert!(home.path().join(".local/share/kio/servicemenus/autosort.desktop").exists());
        }
        assert!(integration::is_installed_in(home.path(), os));
        integration::uninstall_in(home.path(), os).unwrap();
        assert!(!integration::is_installed_in(home.path(), os), "{os}: uninstall removes everything");
    }
    assert!(integration::install_in(Path::new("/tmp"), "plan9", Path::new("/x")).is_err());
}

#[test]
fn windows_registry_commands() {
    let cmds = integration::windows_reg_commands("C:\\Program Files\\AutoSort\\autosort.exe");
    assert_eq!(cmds.len(), 4);
    assert!(cmds.iter().all(|c| c[0] == "add" && c.last().unwrap() == "/f"));
    assert!(cmds.iter().any(|c| c[1].starts_with("HKCU\\Software\\Classes\\*\\shell\\AutoSort\\command")
        && c[4] == "\"C:\\Program Files\\AutoSort\\autosort.exe\" --sort \"%1\""));
    assert!(cmds.iter().any(|c| c[1].contains("Directory\\shell\\AutoSort")));
    assert!(integration::windows_unreg_commands().iter().all(|c| c[0] == "delete"));
}

#[cfg(unix)]
#[test]
fn the_nautilus_script_passes_awkward_paths_through_intact() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("args.txt");
    // a stand-in for autosort that records each argument on its own line, with a space and a quote in its path
    let exe = tmp.path().join("fake 'autosort'");
    fs::write(&exe, format!("#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\n", out.display())).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();

    let script = tmp.path().join("script.sh");
    fs::write(&script, integration::nautilus_script(&exe.to_string_lossy())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let selection = "/home/me/My Docs/report one.pdf\n/home/me/it's here/b.pdf";
    let status = Command::new(&script).env("NAUTILUS_SCRIPT_SELECTED_FILE_PATHS", selection).status().unwrap();
    assert!(status.success());
    let got = fs::read_to_string(&out).unwrap();
    let args: Vec<&str> = got.lines().collect();
    assert_eq!(args, vec!["--sort", "/home/me/My Docs/report one.pdf", "/home/me/it's here/b.pdf"]);

    // and the KDE entry quotes the program the same way
    let kde = integration::kde_service_menu("/opt/a b/autosort");
    assert!(kde.contains("Exec='/opt/a b/autosort' --sort %F"), "{kde}");
}
