use autosort_core::ActivityEntry;

/// Shows one summary notification for a batch of activity (never one per file).
pub fn summarize(entries: &[ActivityEntry]) {
    let done = entries.iter().filter(|e| e.success && !e.dry_run).count();
    let failed = entries.iter().filter(|e| !e.success).count();
    if done == 0 && failed == 0 {
        return;
    }
    let body = match (done, failed) {
        (d, 0) if d == 1 => format!("{}: {}", entries[0].file, entries[0].detail),
        (d, 0) => format!("{d} actions completed. Open AutoSort → Activity to review or undo."),
        (0, f) => format!("{f} action(s) failed. Open AutoSort → Activity for details."),
        (d, f) => format!("{d} actions completed, {f} failed. See Activity."),
    };
    // Notification backends can block (D-Bus, macOS permission prompts), so never do it on the caller's thread.
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().summary("AutoSort").body(&body).show();
    });
}
