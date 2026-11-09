//! egui's own widgets (text boxes, scroll bars, combo boxes, tooltips) in the suite's colours and sizes. The user
//! doesn't see well, so text starts larger than egui's default and targets are roomy.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

use crate::fonts::Weight;
use crate::theme::{nested, Tokens, OUTER_RADIUS};

pub const BODY_SIZE: f32 = 16.0;

pub fn apply(ctx: &egui::Context, th: &Tokens) {
    ctx.global_style_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(BODY_SIZE, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(BODY_SIZE, Weight::SemiBold.family())),
            (TextStyle::Heading, FontId::new(26.0, Weight::Bold.family())),
            (TextStyle::Monospace, FontId::new(14.5, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.interact_size.y = 40.0;
        style.spacing.scroll.bar_width = 8.0;
        let mut v = if th.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = th.bg;
        v.window_fill = th.surface;
        v.extreme_bg_color = th.raised;
        v.faint_bg_color = th.raised;
        v.override_text_color = Some(th.text);
        v.hyperlink_color = th.accent;
        v.selection.bg_fill = th.accent_soft;
        v.selection.stroke = Stroke::new(1.5, th.accent);
        v.window_corner_radius = CornerRadius::same(OUTER_RADIUS as u8);
        v.menu_corner_radius = CornerRadius::same(14);
        let control = CornerRadius::same(nested(OUTER_RADIUS, 8.0) as u8);
        let edge = if th.high_contrast { Stroke::new(1.5, th.border) } else { Stroke::new(1.0, th.border) };
        for (w, fill) in [
            (&mut v.widgets.noninteractive, th.surface),
            (&mut v.widgets.inactive, th.raised),
            (&mut v.widgets.hovered, th.accent_soft),
            (&mut v.widgets.active, th.accent_soft),
            (&mut v.widgets.open, th.accent_soft),
        ] {
            w.corner_radius = control;
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.fg_stroke = Stroke::new(1.5, th.text);
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, th.border.gamma_multiply(0.5));
        v.widgets.inactive.bg_stroke = edge;
        v.widgets.hovered.bg_stroke = Stroke::new(1.5, th.accent);
        v.widgets.active.bg_stroke = Stroke::new(2.0, th.accent);
        v.text_cursor.stroke = Stroke::new(2.0, th.accent);
        v.window_shadow.color = Color32::from_black_alpha(if th.dark { 90 } else { 30 });
        style.visuals = v;
    });
}
