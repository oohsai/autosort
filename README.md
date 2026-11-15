# AutoSort

A Hazel-style file automation app in Rust — **one codebase for macOS, Windows and Linux**.
Watch folders, define *rules* (conditions → actions), and AutoSort files things for you in the background.

## What it does

| | |
|---|---|
| **Conditions** | name (contains / starts / ends / equals / **regex**), extension list (`jpg, png, heic`), kind (image, video, PDF, archive, installer…), size (MB), days since modified / created. Match **all** or **any**. |
| **Actions** | move, copy, rename (keeps extension), move to Trash/Recycle Bin, run a shell command. Chain several per rule. |
| **Smart destinations** | tokens in paths & names: `{name} {filename} {ext} {kind} {year} {month} {monthName} {day} {date} {today} {folder}` — e.g. `~/Pictures/{year}/{monthName}`. |
| **Conflicts** | per action: keep both (`report 2.pdf`), skip, or replace. |
| **Safety** | **Preview** shows exactly what would happen without touching files. Files still being written (recent mtime, `.crdownload`, `.part`, `.tmp`…) and hidden files are left alone. A history stops copy/rename rules from looping. A lock stops two copies running. |
| **Undo** | Every move, copy and rename is reversible from the **Activity** page (copies go to the Trash). Undo never overwrites: if the original spot is taken or the file changed since, it refuses and says why. Restored files are not re-sorted. |
| **Notifications** | One summary notification per batch ("12 actions completed…"), never one per file. Toggle in Settings. |
| **Tray / menu bar** | macOS and Windows: open, pause, run now, quit. Closing the window minimises it; use the tray's *Quit AutoSort* to exit. On Linux use `--headless`. |
| **Runs how you like** | desktop app, `--headless` daemon, or `--once` from a terminal / cron. Optional launch-at-login. |

Rules run top-to-bottom on each file. The first matching rule wins unless you tick **Keep going**.

## Build & run

Needs only a Rust toolchain ([rustup.rs](https://rustup.rs)). No other dependencies to download.

```bash
cargo run --release                      # desktop app
cargo run --release -- --once --dry-run  # preview from the terminal
cargo run --release -- --headless        # background daemon, no window
cargo test -p autosort-core              # engine tests
```

### macOS app bundle

```bash
./scripts/bundle-macos.sh                # -> build/AutoSort.app (native arch)
UNIVERSAL=1 ./scripts/bundle-macos.sh    # Apple Silicon + Intel
```

Drag `build/AutoSort.app` into `/Applications`. The first time it touches Downloads/Desktop/Documents, macOS asks
for permission — allow it (or grant Full Disk Access in System Settings → Privacy & Security).
The bundle is ad-hoc signed; to share it with others, sign with a Developer ID and notarise.
`.github/workflows/build.yml` builds macOS (universal), Windows and Linux artifacts on GitHub.

## Quick start

1. Open the app → **Set up my Downloads folder with starter rules** (or **+ Add folder…**).
2. Open a rule to edit it, **Preview** to check, then leave the app running (or enable launch-at-login).

Starter rules: trash installers older than 14 days · file screenshots into `Sorted/Screenshots/<year>-<month>` ·
everything else with a known type into `Sorted/<Kind>`.

## Layout

```
crates/autosort-core   rules engine, folder watcher service, config store (GUI-free, fully tested)
crates/autosort        egui desktop app + --headless / --once CLI
scripts/bundle-macos.sh, resources/Info.plist   macOS packaging
```

Config, activity log and history live in the OS config dir (`~/Library/Application Support/AutoSort`,
`%APPDATA%\AutoSort`, `~/.config/AutoSort`); set `AUTOSORT_HOME` to override.

## Not yet built

Finder tags and content-based conditions (file text, EXIF), per-rule "ignore" lists, tray icon on Linux, true hide-to-tray.
