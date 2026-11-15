# AutoSort

A Hazel-style file automation app in Rust — **one codebase for macOS, Windows and Linux**.
Watch folders, define *rules* (conditions → actions), and AutoSort files things for you in the background.

## What it does

| | |
|---|---|
| **Conditions** | name (contains / starts / ends / equals / **regex**), extension list (`jpg, png, heic`), kind, size, days since modified / created / **last opened** · **downloaded from** (the site a file came from) · **photo date taken** (EXIF) · **contents** (text inside plain text, PDF, Word/PowerPoint/Excel, OpenDocument) · **Finder tags** · **duplicate** (identical content, including files you already sorted). Match **all** or **any**. |
| **Actions** | move, copy, rename, **add Finder tags** (colour names like `Red` become Finder labels on macOS), **open with an app**, **run a macOS Shortcut**, move to Trash/Recycle Bin, run a shell command. Chain several per rule. |
| **Smart renaming** | tokens plus filters: `{name\|kebab}`, `{name\|lower\|replace:_: }`, `{name\|slice:0:20}`, `{name\|regex:\d+:N}`, `{counter:3}` (next free number), `{source}` (website), `{takenYear}/{takenMonthName}` (photo date). |
| **Schedules** | a rule can run *always*, *daily at 09:00* or *weekly on Sunday at 08:00* — e.g. "archive files older than 30 days every Sunday". **Run now** forces scheduled rules. |
| **Learning mode** | new, imported and starter rules only **preview** until you press **Approve**, so a bad rule can't touch real files. (Turn off in Settings.) |
| **Ignore lists** | per-folder patterns (`*.psd`, `draft*`, `Archive/`, `photos/raw/*`) that rules never touch. |
| **Stats** | each rule shows how many files it has handled and when it last ran. |
| **Import / export** | save a folder's rules to a file and load them elsewhere (fresh ids, learning mode respected). |
| **Right-click in Finder** | Settings → *File manager* → **Install** adds **Sort with AutoSort** (macOS Quick Action, Windows Explorer, Nautilus/Dolphin). It sorts the selection with the rules of the watched folder it is in — immediately, ignoring the settle delay. Same as `autosort --sort PATH…`. |
| **Undo** | every move, copy and rename is reversible from **Activity** (copies go to the Trash). Undo never overwrites; restored files are not re-sorted. |
| **Notifications** | one summary per batch, never one per file. |
| **Tray / menu bar** | macOS and Windows: open, pause, run now, quit. Closing the window minimises it; use the tray's *Quit AutoSort* to exit. On Linux use `--headless`. |
| **Safety** | **Preview** shows what would happen without touching files. Files still being written, hidden files and partial downloads are left alone. History is **per rule**, so copy/rename rules can't loop while other rules (like a scheduled cleanup) can still see a file. Only one copy runs at a time. |
| **Runs how you like** | desktop app, `--headless` daemon, `--once` from a terminal / cron, or `--sort` from your file manager. Optional launch-at-login. |

Rules run top-to-bottom on each file. The first matching rule wins unless you tick **Keep going**.

## Build & run

Needs only a Rust toolchain ([rustup.rs](https://rustup.rs)). No other dependencies to download.

```bash
cargo run --release                      # desktop app
cargo run --release -- --once --dry-run  # preview from the terminal
cargo run --release -- --headless        # background daemon, no window
cargo test --workspace                   # engine + command-line tests (89)
cargo run --release -- --sort ~/Downloads/some.pdf   # sort a file now
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

1. Open the app → **Add a folder** (or drag a folder onto the window). A popup lets you browse or quick-pick Downloads, Desktop or Documents, and optionally start with the sorting rules.
2. **Edit** a rule (opens in a popup), **Preview** to check, then leave the app running (or enable launch-at-login).

The interface is deliberately minimal: a near-black base, one mint accent, and plain-English summaries of every rule.

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

Tray icon on Linux, true hide-to-tray, unzip/compress/convert actions, a rules gallery.
