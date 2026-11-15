//! System tray / menu-bar icon (macOS and Windows).
//!
//! Linux has no tray here: it would need GTK/AppIndicator system libraries. On Linux use `--headless`
//! (e.g. as a systemd user service) for background operation instead.

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use eframe::egui;

/// What the user picked in the tray menu.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub enum TrayAction {
    Open,
    SetPaused(bool),
    RunNow,
    Quit,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use std::sync::mpsc::{self, Receiver};

    use eframe::egui;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use super::TrayAction;

    pub struct Tray {
        _icon: TrayIcon,
        open: MenuId,
        run: MenuId,
        quit: MenuId,
        pause: CheckMenuItem,
        rx: Receiver<MenuId>,
    }

    /// A small "sorting" glyph: three bars of decreasing length on a rounded blue square.
    fn icon() -> Option<Icon> {
        const N: usize = 32;
        let mut px = vec![0u8; N * N * 4];
        let inside = |x: i32, y: i32| {
            // rounded square, radius 7
            let (cx, cy) = (x.clamp(7, N as i32 - 8), y.clamp(7, N as i32 - 8));
            (x - cx).pow(2) + (y - cy).pow(2) <= 49
        };
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                if !inside(x, y) {
                    continue;
                }
                let bar = |top: i32, width: i32| (top..top + 4).contains(&y) && (6..6 + width).contains(&x);
                let white = bar(8, 20) || bar(14, 14) || bar(20, 8);
                let c = if white { [255, 255, 255, 255] } else { [59, 130, 246, 255] };
                let i = (y as usize * N + x as usize) * 4;
                px[i..i + 4].copy_from_slice(&c);
            }
        }
        Icon::from_rgba(px, N as u32, N as u32).ok()
    }

    impl Tray {
        /// Must be called on the main thread once the event loop is running (inside eframe's app creator).
        pub fn new(ctx: &egui::Context) -> Option<Tray> {
            let menu = Menu::new();
            let open = MenuItem::new("Open AutoSort", true, None);
            let pause = CheckMenuItem::new("Pause automation", true, false, None);
            let run = MenuItem::new("Run all rules now", true, None);
            let quit = MenuItem::new("Quit AutoSort", true, None);
            menu.append_items(&[&open, &pause, &run, &PredefinedMenuItem::separator(), &quit]).ok()?;

            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
                let _ = tx.send(ev.id);
                repaint.request_repaint();
            }));

            let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("AutoSort");
            if let Some(i) = icon() {
                builder = builder.with_icon(i);
            }
            let tray = builder.build().map_err(|e| eprintln!("autosort: tray icon unavailable: {e}")).ok()?;
            Some(Tray { _icon: tray, open: open.id().clone(), run: run.id().clone(), quit: quit.id().clone(), pause, rx })
        }

        pub fn poll(&self) -> Vec<TrayAction> {
            let mut out = Vec::new();
            while let Ok(id) = self.rx.try_recv() {
                if id == self.open {
                    out.push(TrayAction::Open);
                } else if id == self.run {
                    out.push(TrayAction::RunNow);
                } else if id == self.quit {
                    out.push(TrayAction::Quit);
                } else if id == *self.pause.id() {
                    out.push(TrayAction::SetPaused(self.pause.is_checked()));
                }
            }
            out
        }

        pub fn set_paused(&self, paused: bool) {
            self.pause.set_checked(paused);
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::*;

    pub struct Tray;

    impl Tray {
        pub fn new(_ctx: &egui::Context) -> Option<Tray> {
            None
        }
        pub fn poll(&self) -> Vec<TrayAction> {
            Vec::new()
        }
        pub fn set_paused(&self, _paused: bool) {}
    }
}

pub use imp::Tray;
