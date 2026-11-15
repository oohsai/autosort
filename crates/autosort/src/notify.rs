use std::thread::JoinHandle;

use autosort_core::ActivityEntry;

/// Shows one summary notification for a batch of activity (never one per file). Returns the thread so a
/// short-lived process (the right-click action) can wait for the notification to actually appear.
pub fn summarize(entries: &[ActivityEntry]) -> JoinHandle<()> {
    let done = entries.iter().filter(|e| e.success && !e.dry_run).count();
    let failed = entries.iter().filter(|e| !e.success).count();
    if done == 0 && failed == 0 {
        return std::thread::spawn(|| {});
    }
    let body = match (done, failed) {
        (d, 0) if d == 1 => format!("{}: {}", entries[0].file, entries[0].detail),
        (d, 0) => format!("{d} actions completed. Open AutoSort → Activity to review or undo."),
        (0, f) => format!("{f} action(s) failed. Open AutoSort → Activity for details."),
        (d, f) => format!("{d} actions completed, {f} failed. See Activity."),
    };
    message("AutoSort", &body)
}

/// A plain notification. Backends can block (D-Bus, macOS permission prompts), so it never runs on the
/// caller's thread.
pub fn message(title: &str, body: &str) -> JoinHandle<()> {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().summary(&title).body(&body).show();
    })
}
