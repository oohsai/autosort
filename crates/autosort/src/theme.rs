//! The look of AutoSort: two colours — a near-black "ink" and one mint accent — plus neutral greys.
//! Everything in the UI draws from these constants; nothing else introduces a hue.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, vec2};

// Ink (neutral surfaces, darkest to lightest)
pub const SIDEBAR: Color32 = Color32::from_rgb(0x0F, 0x10, 0x12);
pub const BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
pub const INPUT: Color32 = Color32::from_rgb(0x0C, 0x0D, 0x0F);
pub const SURFACE: Color32 = Color32::from_rgb(0x1B, 0x1C, 0x20);
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x25, 0x27, 0x2C);
pub const BORDER: Color32 = Color32::from_rgb(0x2B, 0x2D, 0x32);
pub const BORDER_HI: Color32 = Color32::from_rgb(0x3A, 0x3D, 0x44);

// Text (neutral)
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEA);
pub const DIM: Color32 = Color32::from_rgb(0x91, 0x94, 0x9B);
pub const FAINT: Color32 = Color32::from_rgb(0x5F, 0x62, 0x69);

// The one accent
pub const ACCENT: Color32 = Color32::from_rgb(0x5B, 0xE3, 0xA8);
pub const ACCENT_HI: Color32 = Color32::from_rgb(0x7E, 0xEE, 0xBD);
pub const ACCENT_ON: Color32 = Color32::from_rgb(0x07, 0x16, 0x0F);
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x0D, 0x2A, 0x1F, 0x40);

pub const RADIUS: u8 = 8;

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER_HI);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = egui::Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) };
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = SURFACE;
    v.weak_text_color = Some(DIM);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0x5B, 0xE3, 0xA8, 70);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.text_cursor.stroke = Stroke::new(1.5, ACCENT);

    let r = CornerRadius::same(RADIUS);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG;
    w.noninteractive.weak_bg_fill = BG;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = SURFACE;
    w.inactive.weak_bg_fill = SURFACE;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = SURFACE_HI;
    w.hovered.weak_bg_fill = SURFACE_HI;
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER_HI);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.corner_radius = r;

    w.active.bg_fill = SURFACE_HI;
    w.active.weak_bg_fill = SURFACE_HI;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, TEXT);
    w.active.corner_radius = r;

    w.open.bg_fill = SURFACE_HI;
    w.open.weak_bg_fill = SURFACE_HI;
    w.open.bg_stroke = Stroke::new(1.0, BORDER_HI);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = r;

    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.interact_size.y = 32.0;
        s.spacing.combo_width = 140.0;
        s.spacing.menu_margin = egui::Margin::same(8);
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.floating = true;
        s.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
    });
}
