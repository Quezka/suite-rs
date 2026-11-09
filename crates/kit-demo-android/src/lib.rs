//! The kit's glass on a phone: a day of coloured events scrolls by itself, up and down, under a glass top bar and a
//! glass tab bar, so frame times can be read from logcat (`adb logcat -s quezka`) without touching the screen.
//! Touching it stops the automatic scroll.

use egui::{Align2, Color32, CornerRadius, Order, Pos2, Rect, Sense, Vec2};
use quezka_kit::fonts::Weight;
use quezka_kit::glass::{self, Weight as Glass};
use quezka_kit::theme::{nested, CARD_PADDING, GAP, OUTER_RADIUS};
use quezka_kit::{App, Tokens};

const EVENTS: [(&str, &str, u32); 12] = [
    ("Mathematics", "08:30 · Room B1", 0x4f7cff),
    ("Biology", "09:30 · Lab 2", 0x16a34a),
    ("Lunch with Sam", "12:00 · Canteen", 0xb45309),
    ("Chemistry", "13:00 · Lab 1", 0xe11d48),
    ("Study group", "16:00 · Library", 0x0891b2),
    ("Gym", "18:00 · Cadence: legs", 0x7c3aed),
    ("Dinner", "20:00 · Home", 0xdb2777),
    ("Read Frankenstein", "21:00 · ch. 5–6", 0x0d9488),
    ("Bed", "23:00 · Vesper", 0x4338ca),
    ("Ride to school", "07:40 · Pillion", 0x0369a1),
    ("Water", "1.5 l so far", 0x0e7490),
    ("Money", "€12.40 today", 0x15803d),
];

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

#[derive(Default)]
pub struct Demo {
    touched: bool,
    tab: usize,
    note: String,
}

impl App for Demo {
    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        let ctx = root.ctx().clone();
        let screen = ctx.content_rect();
        let time = ctx.input(|i| i.time) as f32;
        if ctx.input(|i| i.pointer.any_down()) {
            self.touched = true;
        }
        let safe = quezka_kit::safe_area(&ctx);
        let top = f32::from(safe.top);
        let bottom = f32::from(safe.bottom);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg)).show(root, |ui| {
            let mut scroll = egui::ScrollArea::vertical();
            if !self.touched {
                // Up and down, smoothly, forever: a steady load for measuring.
                let range = 900.0;
                scroll = scroll.vertical_scroll_offset(range * 0.5 * (1.0 - (time * 0.9).cos()));
                ctx.request_repaint();
            }
            scroll.show(ui, |ui| {
                ui.add_space(top + 64.0 + 2.0 * GAP);
                // Somewhere to try the keyboard: Russian, Italian, autocorrect, swipe typing.
                ui.horizontal(|ui| {
                    ui.add_space(GAP);
                    let field = egui::TextEdit::multiline(&mut self.note)
                        .hint_text("Type something…")
                        .desired_width(ui.available_width() - GAP)
                        .desired_rows(2)
                        .font(Weight::Medium.font(17.0));
                    if ui.add(field).has_focus() {
                        self.touched = true;
                    }
                });
                ui.add_space(GAP);
                for round in 0..2 {
                    for (title, detail, colour) in EVENTS {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 96.0), Sense::hover());
                        let rect = rect.shrink2(Vec2::new(GAP, 0.0));
                        let p = ui.painter();
                        p.rect_filled(rect, CornerRadius::same(OUTER_RADIUS as u8), th.surface);
                        let inner = rect.shrink(CARD_PADDING);
                        p.rect_filled(inner, CornerRadius::same(nested(OUTER_RADIUS, CARD_PADDING) as u8), hex(colour));
                        p.text(inner.left_top() + Vec2::new(14.0, 12.0), Align2::LEFT_TOP, title, Weight::Bold.font(19.0), Color32::WHITE);
                        p.text(inner.left_top() + Vec2::new(14.0, 40.0), Align2::LEFT_TOP, format!("{detail}{}", if round == 1 { " · tomorrow" } else { "" }), Weight::Medium.font(14.0), Color32::from_white_alpha(235));
                        ui.add_space(GAP);
                    }
                }
                ui.add_space(110.0);
            });
        });

        egui::Area::new(egui::Id::NULL.with("chrome")).order(Order::Foreground).fade_in(false).fixed_pos(Pos2::ZERO).show(&ctx, |ui| {
            let p = ui.painter();
            let bar = Rect::from_min_max(Pos2::new(screen.min.x + GAP, screen.min.y + top + GAP), Pos2::new(screen.max.x - GAP, screen.min.y + top + GAP + 64.0));
            glass::pane(p, bar, OUTER_RADIUS, th, Glass::Regular, true);
            p.text(bar.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, "Monday, 9 November", Weight::Bold.font(22.0), th.text);

            let tabs = Rect::from_min_max(Pos2::new(screen.min.x + GAP, screen.max.y - bottom - GAP - 64.0), Pos2::new(screen.max.x - GAP, screen.max.y - bottom - GAP));
            glass::pane(p, tabs, tabs.height() / 2.0, th, Glass::Regular, true);
            let names = ["Today", "Week", "Tasks", "Notes"];
            let width = tabs.width() / names.len() as f32;
            for (i, name) in names.into_iter().enumerate() {
                let cell = Rect::from_min_size(Pos2::new(tabs.min.x + width * i as f32, tabs.min.y), Vec2::new(width, tabs.height()));
                if i == self.tab {
                    p.rect_filled(cell.shrink(8.0), CornerRadius::same(((tabs.height() - 16.0) / 2.0) as u8), th.accent_soft);
                }
                p.text(cell.center(), Align2::CENTER_CENTER, name, Weight::SemiBold.font(15.0), th.text);
                if ui.interact(cell, ui.scope_id().with(("tab", i)), Sense::click()).clicked() {
                    self.tab = i;
                }
            }
        });
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    if let Err(e) = quezka_kit::run_android(app, "Kit demo", Demo::default()) {
        quezka_kit::log(&format!("stopped: {e}"));
    }
}
