//! desktop app pieces (quire first): width classes, labelled fields, dialog, menu, date + time picker
//! big + high contrast by default <- user sight; every hit target >= `48` px

use chrono::{Datelike, Duration, NaiveDate};
use egui::{Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::fonts::Weight;
use crate::icons::icon;
use crate::theme::Tokens;
use crate::widgets::{fit, text};

pub const FIELD_HEIGHT: f32 = 48.0;
pub const FIELD_RADIUS: u8 = 12;

/// window width class (material 3): layout picks columns + nav by it
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WidthClass {
    /// < `600`: one column, bottom bar | drawer
    Compact,
    /// < `1000`: rail + one column (half 1366 screen lands here)
    Medium,
    /// >= `1000`: sidebar + 2 columns
    Expanded,
}

pub fn width_class(width: f32) -> WidthClass {
    if width < 600.0 {
        WidthClass::Compact
    } else if width < 1000.0 {
        WidthClass::Medium
    } else {
        WidthClass::Expanded
    }
}

/// columns of >= `min_width` that fit `width` (1..=`max`)
pub fn columns_for(width: f32, min_width: f32, max: usize) -> usize {
    ((width / min_width).floor() as usize).clamp(1, max.max(1))
}

fn field_frame(th: &Tokens, focused: bool, margin: Margin) -> Frame {
    let stroke = if focused {
        Stroke::new(2.0, th.accent)
    } else {
        Stroke::new(if th.high_contrast { 1.5 } else { 1.0 }, th.border)
    };
    Frame::new().fill(th.surface).stroke(stroke).corner_radius(CornerRadius::same(FIELD_RADIUS)).inner_margin(margin)
}

fn caption(ui: &mut Ui, th: &Tokens, label: &str) {
    if !label.is_empty() {
        ui.label(text(label, 14.0, th.muted, Weight::SemiBold));
        ui.add_space(4.0);
    }
}

/// labelled 1-line field; label "" -> none
pub fn field(ui: &mut Ui, th: &Tokens, label: &str, value: &mut String, hint: &str) -> Response {
    caption(ui, th, label);
    let id = ui.next_auto_id();
    let focused = ui.memory(|m| m.has_focus(id.with("edit")));
    let inner = field_frame(th, focused, Margin::symmetric(14, 2)).show(ui, |ui| {
        let output = egui::TextEdit::singleline(value).id(id.with("edit")).hint_text(hint).frame(egui::Frame::NONE).margin(Margin::ZERO)
            .font(FontId::new(16.0, Weight::Regular.family())).text_color(th.text).desired_width(f32::INFINITY)
            .min_size(Vec2::new(0.0, FIELD_HEIGHT - 4.0)).align(egui::Align2::LEFT_CENTER).show(ui);
        crate::typing::record(ui, &output, value);
        output.response.response
    });
    ui.add_space(8.0);
    inner.inner
}

/// labelled multi-line field
pub fn area(ui: &mut Ui, th: &Tokens, label: &str, value: &mut String, hint: &str, rows: usize) -> Response {
    caption(ui, th, label);
    let id = ui.next_auto_id();
    let focused = ui.memory(|m| m.has_focus(id.with("edit")));
    let inner = field_frame(th, focused, Margin::symmetric(14, 12)).show(ui, |ui| {
        let output = egui::TextEdit::multiline(value).id(id.with("edit")).hint_text(hint).frame(egui::Frame::NONE).margin(Margin::ZERO)
            .font(FontId::new(16.0, Weight::Regular.family())).text_color(th.text).desired_width(f32::INFINITY)
            .desired_rows(rows).show(ui);
        crate::typing::record(ui, &output, value);
        output.response.response
    });
    ui.add_space(8.0);
    inner.inner
}

/// button sized to words (dialog rows, toolbars); `Kind` as `widgets::button`
pub fn inline_button(ui: &mut Ui, th: &Tokens, label: &str, kind: crate::widgets::Kind) -> bool {
    use crate::widgets::Kind;
    let (fill, ink) = match kind {
        Kind::Filled => (th.accent, th.on_accent),
        Kind::Tonal => (th.accent_soft, th.on_accent_soft),
        Kind::Outlined | Kind::Text => (Color32::TRANSPARENT, th.accent),
        Kind::Danger => (th.danger, if th.dark { th.bg } else { th.surface }),
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::new(15.0, Weight::SemiBold.family()), ink);
    let size = Vec2::new((galley.size().x + 40.0).max(72.0), FIELD_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let round = CornerRadius::same(24);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect.shrink(2.0), round, fill);
    if kind == Kind::Outlined || (th.high_contrast && kind != Kind::Text) {
        painter.rect_stroke(rect.shrink(2.0), round, Stroke::new(1.2, th.border), StrokeKind::Inside);
    }
    if response.hovered() || response.is_pointer_button_down_on() {
        painter.rect_filled(rect.shrink(2.0), round, ink.gamma_multiply(if response.is_pointer_button_down_on() { 0.14 } else { 0.08 }));
    }
    painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// what happened to a dialog this frame
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DialogOutcome {
    /// esc | backdrop click -> caller closes
    pub dismissed: bool,
}

/// modal dialog: title + body; width <= `520`, fits small screens (scrolls)
pub fn dialog(ctx: &egui::Context, th: &Tokens, id: Id, title: &str, body: impl FnOnce(&mut Ui)) -> DialogOutcome {
    let screen = ctx.content_rect();
    let width = (screen.width() - 32.0).clamp(240.0, 520.0);
    let frame = Frame::new().fill(th.surface).corner_radius(CornerRadius::same(28)).inner_margin(Margin::same(24))
        .stroke(if th.high_contrast { Stroke::new(2.0, th.border) } else { Stroke::NONE });
    let response = egui::Modal::new(id).frame(frame).backdrop_color(Color32::from_black_alpha(if th.dark { 150 } else { 100 }))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.label(text(title, 22.0, th.text, Weight::Bold));
            ui.add_space(12.0);
            egui::ScrollArea::vertical().max_height((screen.height() - 160.0).max(120.0)).auto_shrink([false, true]).show(ui, body);
        });
    DialogOutcome { dismissed: response.should_close() }
}

/// right-aligned row of dialog buttons; last = main action -> index tapped
pub fn dialog_buttons(ui: &mut Ui, th: &Tokens, buttons: &[(&str, crate::widgets::Kind)]) -> Option<usize> {
    ui.add_space(12.0);
    let mut tapped = None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        for (i, (label, kind)) in buttons.iter().enumerate().rev() {
            if inline_button(ui, th, label, *kind) {
                tapped = Some(i);
            }
        }
    });
    tapped
}

/// popup menu under `anchor` (button | row); `items` draws `menu_item`s
pub fn menu(th: &Tokens, anchor: &Response, items: impl FnOnce(&mut Ui)) {
    let frame = Frame::new().fill(th.surface).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(6))
        .stroke(Stroke::new(if th.high_contrast { 1.5 } else { 1.0 }, th.border))
        .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(50) });
    egui::Popup::menu(anchor).frame(frame).width(240.0).show(|ui| {
        ui.set_min_width(220.0);
        items(ui)
    });
}

/// same menu on right click
pub fn context_menu(th: &Tokens, anchor: &Response, items: impl FnOnce(&mut Ui)) {
    let frame = Frame::new().fill(th.surface).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(6))
        .stroke(Stroke::new(if th.high_contrast { 1.5 } else { 1.0 }, th.border));
    egui::Popup::context_menu(anchor).frame(frame).width(240.0).show(|ui| {
        ui.set_min_width(220.0);
        items(ui)
    });
}

/// menu row; tap -> closes menu + true
pub fn menu_item(ui: &mut Ui, th: &Tokens, glyph: Option<&'static str>, label: &str, danger: bool) -> bool {
    let ink = if danger { th.danger } else { th.text };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(8), th.hover);
    }
    let mut x = rect.left() + 12.0;
    if let Some(name) = glyph {
        icon(&painter, Rect::from_center_size(Pos2::new(x + 10.0, rect.center().y), Vec2::splat(20.0)), name, false, ink);
        x += 32.0;
    }
    let galley = fit(ui, label, 15.0, 12.0, Weight::Medium, ink, rect.right() - x - 8.0);
    painter.galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, ink);
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap_or(d)
}

/// 6 weeks x 7 days shown for month of `month`, monday first
pub fn month_grid(month: NaiveDate) -> Vec<NaiveDate> {
    let first = first_of_month(month);
    let start = first - Duration::days(first.weekday().num_days_from_monday() as i64);
    (0..42).map(|i| start + Duration::days(i)).collect()
}

fn round_icon(ui: &mut Ui, th: &Tokens, glyph: &'static str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::click());
    if response.hovered() {
        ui.painter().circle_filled(rect.center(), 21.0, th.hover);
    }
    icon(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(24.0)), glyph, false, th.text);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// month calendar; tap day -> `value` = day + true. `month_name(1..=12)`, `weekdays` = mon..sun short names
pub fn date_picker(ui: &mut Ui, th: &Tokens, id: Id, value: &mut NaiveDate, today: NaiveDate,
                   month_name: &dyn Fn(u32) -> String, weekdays: &[String; 7]) -> bool {
    let shown_id = id.with("month");
    let mut shown: NaiveDate = ui.data(|d| d.get_temp(shown_id)).unwrap_or_else(|| first_of_month(*value));
    let mut changed = false;
    ui.horizontal(|ui| {
        if round_icon(ui, th, "chevron-left") {
            shown = add_months(shown, -1);
        }
        let heading = format!("{} {}", month_name(shown.month()), shown.year());
        let w = ui.available_width() - 48.0;
        ui.allocate_ui_with_layout(Vec2::new(w, 44.0), Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
            ui.label(text(heading, 17.0, th.text, Weight::SemiBold));
        });
        if round_icon(ui, th, "chevron-right") {
            shown = add_months(shown, 1);
        }
    });
    let cell = ((ui.available_width()) / 7.0).clamp(36.0, 52.0);
    let grid_width = cell * 7.0;
    let (head, _) = ui.allocate_exact_size(Vec2::new(grid_width, 28.0), Sense::hover());
    for (i, name) in weekdays.iter().enumerate() {
        let r = Rect::from_min_size(head.min + Vec2::new(cell * i as f32, 0.0), Vec2::new(cell, 28.0));
        let g = fit(ui, name, 13.0, 10.0, Weight::SemiBold, th.muted, cell - 4.0);
        ui.painter().galley(r.center() - g.size() / 2.0, g, th.muted);
    }
    let (grid, _) = ui.allocate_exact_size(Vec2::new(grid_width, cell * 6.0), Sense::hover());
    for (i, day) in month_grid(shown).into_iter().enumerate() {
        let r = Rect::from_min_size(grid.min + Vec2::new(cell * (i % 7) as f32, cell * (i / 7) as f32), Vec2::splat(cell));
        let response = ui.interact(r, id.with(("day", i)), Sense::click());
        let chosen = day == *value;
        let inside = day.month() == shown.month();
        let painter = ui.painter_at(r);
        let disc = r.shrink(3.0);
        if chosen {
            painter.circle_filled(disc.center(), disc.width() / 2.0, th.accent);
        } else if response.hovered() {
            painter.circle_filled(disc.center(), disc.width() / 2.0, th.hover);
        }
        if day == today && !chosen {
            painter.circle_stroke(disc.center(), disc.width() / 2.0 - 1.0, Stroke::new(1.5, th.accent));
        }
        let ink = if chosen { th.on_accent } else if inside { th.text } else { th.faint };
        let g = ui.painter().layout_no_wrap(day.day().to_string(), FontId::new(15.0, Weight::Medium.family()), ink);
        painter.galley(r.center() - g.size() / 2.0, g, ink);
        if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *value = day;
            shown = first_of_month(day);
            changed = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(shown_id, shown));
    changed
}

/// clamp + step minutes of day (`0..1440`); step wraps (23:55 + 5 -> 00:00)
pub fn step_minutes(minutes: i32, delta: i32) -> i32 {
    (minutes + delta).rem_euclid(24 * 60)
}

fn stepper(ui: &mut Ui, th: &Tokens, value: &str) -> i32 {
    let mut delta = 0;
    ui.vertical(|ui| {
        if round_icon(ui, th, "chevron-up") {
            delta = 1;
        }
        let (r, _) = ui.allocate_exact_size(Vec2::new(44.0, 40.0), Sense::hover());
        let g = ui.painter().layout_no_wrap(value.to_string(), FontId::new(26.0, Weight::Bold.family()), th.text);
        ui.painter().galley(r.center() - g.size() / 2.0, g, th.text);
        if round_icon(ui, th, "chevron-down") {
            delta = -1;
        }
    });
    delta
}

/// hh:mm picker: hour +-1, minute +-`step`; scroll wheel over hours/minutes works too
pub fn time_picker(ui: &mut Ui, th: &Tokens, minutes: &mut i32, step: i32) -> bool {
    let before = *minutes;
    ui.horizontal(|ui| {
        let h = stepper(ui, th, &format!("{:02}", *minutes / 60));
        ui.label(text(":", 26.0, th.text, Weight::Bold));
        let m = stepper(ui, th, &format!("{:02}", *minutes % 60));
        if h != 0 {
            *minutes = step_minutes(*minutes, h * 60);
        }
        let step = step.max(1);
        if m > 0 {
            *minutes = step_minutes(*minutes - *minutes % step, step);
        } else if m < 0 {
            let r = *minutes % step;
            *minutes = step_minutes(*minutes, -(if r == 0 { step } else { r }));
        }
    });
    *minutes != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_classes_and_columns() {
        assert_eq!(width_class(360.0), WidthClass::Compact);
        assert_eq!(width_class(683.0), WidthClass::Medium);
        assert_eq!(width_class(1366.0), WidthClass::Expanded);
        assert_eq!(columns_for(1300.0, 420.0, 2), 2);
        assert_eq!(columns_for(300.0, 420.0, 3), 1);
    }

    #[test]
    fn month_grid_starts_on_monday() {
        let g = month_grid(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap());
        assert_eq!(g[0], NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(g[6], NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
        assert_eq!(g.len(), 42);
        assert_eq!(add_months(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(), -1), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
    }

    #[test]
    fn minutes_wrap() {
        assert_eq!(step_minutes(23 * 60 + 55, 5), 0);
        assert_eq!(step_minutes(0, -5), 23 * 60 + 55);
    }
}
