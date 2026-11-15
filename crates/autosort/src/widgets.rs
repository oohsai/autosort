//! Small, consistent building blocks. All colours come from `theme`.

use eframe::egui::{
    self, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, vec2,
};

use crate::theme as t;

// ------------------------------------------------------------------ containers

/// A subtle bordered surface.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(t::SURFACE)
        .stroke(Stroke::new(1.0, t::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Small caps-style heading above a group.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).small().color(t::FAINT).strong());
}

pub fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t::BORDER);
}

// --------------------------------------------------------------------- buttons

#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// Accent-filled: the one main action on screen.
    Primary,
    /// Outlined: secondary actions.
    Ghost,
    /// Text only, accent coloured: inline "+ Add" links.
    Link,
    /// Text only, neutral.
    Quiet,
    /// Text only, hover reveals intent: destructive.
    Danger,
}

pub fn button(ui: &mut Ui, label: &str, look: Look) -> Response {
    let font = FontId::proportional(14.0);
    let enabled = ui.is_enabled();
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::WHITE);
    let pad = match look {
        Look::Primary | Look::Ghost => vec2(16.0, 0.0),
        _ => vec2(6.0, 0.0),
    };
    let size = vec2(galley.size().x + pad.x * 2.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = resp.hovered() && enabled;
    let down = resp.is_pointer_button_down_on() && enabled;

    let (fill, stroke, text) = match look {
        Look::Primary => (
            if down { t::ACCENT } else if hover { t::ACCENT_HI } else { t::ACCENT },
            Stroke::NONE,
            t::ACCENT_ON,
        ),
        Look::Ghost => (
            if hover { t::SURFACE_HI } else { Color32::TRANSPARENT },
            Stroke::new(1.0, if hover { t::BORDER_HI } else { t::BORDER }),
            t::TEXT,
        ),
        Look::Link => (Color32::TRANSPARENT, Stroke::NONE, if hover { t::ACCENT_HI } else { t::ACCENT }),
        Look::Quiet => (Color32::TRANSPARENT, Stroke::NONE, if hover { t::TEXT } else { t::DIM }),
        Look::Danger => (Color32::TRANSPARENT, Stroke::NONE, if hover { t::TEXT } else { t::DIM }),
    };
    let (fill, text) = if enabled { (fill, text) } else { (if look == Look::Primary { t::SURFACE_HI } else { Color32::TRANSPARENT }, t::FAINT) };

    ui.painter().rect(rect, CornerRadius::same(t::RADIUS), fill, stroke, StrokeKind::Inside);
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(14.0), text);
    let pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(pos, galley, text);
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn primary(ui: &mut Ui, label: &str) -> Response {
    button(ui, label, Look::Primary)
}
pub fn ghost(ui: &mut Ui, label: &str) -> Response {
    button(ui, label, Look::Ghost)
}
pub fn link(ui: &mut Ui, label: &str) -> Response {
    button(ui, label, Look::Link)
}
pub fn quiet(ui: &mut Ui, label: &str) -> Response {
    button(ui, label, Look::Quiet)
}
pub fn danger(ui: &mut Ui, label: &str) -> Response {
    button(ui, label, Look::Danger)
}

/// A pill-shaped option (quick picks, segmented controls).
pub fn chip(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), Color32::WHITE);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 28.0), Sense::click());
    let hover = resp.hovered();
    let (fill, stroke, text) = if selected {
        (t::ACCENT_SOFT, Stroke::new(1.0, t::ACCENT), t::ACCENT)
    } else {
        (if hover { t::SURFACE_HI } else { Color32::TRANSPARENT }, Stroke::new(1.0, if hover { t::BORDER_HI } else { t::BORDER }), t::DIM)
    };
    ui.painter().rect(rect, CornerRadius::same(14), fill, stroke, StrokeKind::Inside);
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), text);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, text);
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Two-or-more option control. Returns true if the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (v, label) in options {
            if chip(ui, label, *value == *v).clicked() && *value != *v {
                *value = *v;
                changed = true;
            }
        }
    });
    changed
}

// ---------------------------------------------------------------------- toggle

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let k = ui.ctx().animate_bool_responsive(resp.id, *on);
    let track = if *on { t::ACCENT } else { t::SURFACE_HI };
    let stroke = if *on { Stroke::NONE } else { Stroke::new(1.0, if resp.hovered() { t::BORDER_HI } else { t::BORDER }) };
    ui.painter().rect(rect, CornerRadius::same(10), track, stroke, StrokeKind::Inside);
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), k);
    let knob = if *on { t::ACCENT_ON } else { t::DIM };
    ui.painter().circle_filled(Pos2::new(x, rect.center().y), 6.5, knob);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// "Label (+ dim description) ........ [toggle]".
pub fn setting_row(ui: &mut Ui, title: &str, hint: &str, on: &mut bool) -> bool {
    let mut changed = false;
    egui::Sides::new().show(
        ui,
        |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).color(t::TEXT));
                if !hint.is_empty() {
                    ui.label(RichText::new(hint).small().color(t::DIM));
                }
            });
        },
        |ui| changed = toggle(ui, on).changed(),
    );
    changed
}

// ----------------------------------------------------------------------- badge

pub fn badge(ui: &mut Ui, text: &str, fg: Color32, bg: Color32, stroke: Stroke) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(11.5), fg);
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 20.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(10), bg, stroke, StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
}

// ----------------------------------------------------------------------- icons

#[derive(Clone, Copy)]
pub enum Icon {
    Folder,
    Activity,
    Sliders,
}

pub fn paint_icon(ui: &Ui, icon: Icon, center: Pos2, color: Color32) {
    let p = ui.painter();
    let s = Stroke::new(1.5, color);
    match icon {
        Icon::Folder => {
            let body = Rect::from_center_size(center + vec2(0.0, 1.5), vec2(16.0, 11.0));
            p.rect_stroke(body, CornerRadius::same(2), s, StrokeKind::Inside);
            p.line_segment([body.left_top() + vec2(0.0, -0.0), body.left_top() + vec2(0.0, -2.5)], s);
            p.line_segment([body.left_top() + vec2(0.0, -2.5), body.left_top() + vec2(5.5, -2.5)], s);
            p.line_segment([body.left_top() + vec2(5.5, -2.5), body.left_top() + vec2(7.0, 0.0)], s);
        }
        Icon::Activity => {
            for (i, w) in [14.0, 10.0, 12.0].into_iter().enumerate() {
                let y = center.y - 5.0 + i as f32 * 5.0;
                p.line_segment([Pos2::new(center.x - 7.0, y), Pos2::new(center.x - 7.0 + w, y)], s);
            }
        }
        Icon::Sliders => {
            for (i, knob) in [-2.5f32, 3.0].into_iter().enumerate() {
                let y = center.y - 3.5 + i as f32 * 7.0;
                p.line_segment([Pos2::new(center.x - 7.0, y), Pos2::new(center.x + 7.0, y)], s);
                p.circle(Pos2::new(center.x + knob, y), 2.6, t::SIDEBAR, s);
            }
        }
    }
}

/// Sidebar row: icon, label, optional status dot; full width, accent bar when selected.
pub fn nav_item(ui: &mut Ui, selected: bool, icon: Icon, label: &str, status: Option<bool>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let hover = resp.hovered();
    if selected || hover {
        ui.painter().rect_filled(rect, CornerRadius::same(8), if selected { t::SURFACE } else { Color32::from_rgb(0x15, 0x16, 0x19) });
    }
    if selected {
        let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 9.0), vec2(3.0, rect.height() - 18.0));
        ui.painter().rect_filled(bar, CornerRadius::same(2), t::ACCENT);
    }
    let fg = if selected { t::TEXT } else if hover { t::TEXT } else { t::DIM };
    paint_icon(ui, icon, rect.left_center() + vec2(24.0, 0.0), if selected { t::ACCENT } else { fg });
    let max_w = rect.width() - 64.0;
    let mut text = label.to_string();
    let mut galley = ui.painter().layout_no_wrap(text.clone(), FontId::proportional(14.0), fg);
    while galley.size().x > max_w && text.chars().count() > 3 {
        text.pop();
        galley = ui.painter().layout_no_wrap(format!("{text}…"), FontId::proportional(14.0), fg);
    }
    ui.painter().galley(Pos2::new(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    if let Some(on) = status {
        ui.painter().circle_filled(rect.right_center() - vec2(16.0, 0.0), 3.0, if on { t::ACCENT } else { t::FAINT });
    }
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Large, calm empty state.
/// `row_width` is the approximate total width of the buttons, used to centre them.
pub fn empty_state(ui: &mut Ui, title: &str, body: &str, row_width: f32, add_buttons: impl FnOnce(&mut Ui)) {
    ui.add_space(36.0);
    ui.vertical_centered(|ui| {
        paint_icon_large(ui);
        ui.add_space(14.0);
        ui.label(RichText::new(title).size(18.0).color(t::TEXT));
        ui.add_space(2.0);
        ui.label(RichText::new(body).color(t::DIM));
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            // center the button row
            let total = ui.available_width();
            ui.add_space(((total - row_width) / 2.0).max(0.0));
            add_buttons(ui);
        });
    });
}

fn paint_icon_large(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(16), t::SURFACE, Stroke::new(1.0, t::BORDER), StrokeKind::Inside);
    let c = rect.center();
    let s = Stroke::new(2.0, t::ACCENT);
    for (i, w) in [22.0, 15.0, 9.0].into_iter().enumerate() {
        let y = c.y - 8.0 + i as f32 * 8.0;
        p.line_segment([Pos2::new(c.x - 11.0, y), Pos2::new(c.x - 11.0 + w, y)], s);
    }
}
