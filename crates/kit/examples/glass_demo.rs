//! A look at the kit's glass: a day of colourful events scrolls under a glass top bar, beside a floating glass sidebar,
//! with a glass button row along the bottom.
//!
//! cargo run --example glass_demo                       # a window
//! cargo run --example glass_demo -- --screenshot x.png [--dark] [--scroll 260]

use egui::{Align2, Color32, CornerRadius, FontId, Order, Pos2, Rect, Sense, Vec2};
use quezka_kit::glass::{self, Weight};
use quezka_kit::theme::{nested, CARD_PADDING, GAP, OUTER_RADIUS};
use quezka_kit::{App, Tokens};

struct Demo {
    dark: Option<bool>,
    scroll: f32,
}

const EVENTS: [(&str, &str, u32); 9] = [
    ("Mathematics", "08:30 · Room B1", 0x4f7cff),
    ("Biology", "09:30 · Lab 2", 0x16a34a),
    ("Lunch with Sam", "12:00 · Canteen", 0xb45309),
    ("Chemistry", "13:00 · Lab 1", 0xe11d48),
    ("Study group", "16:00 · Library", 0x0891b2),
    ("Gym", "18:00 · Cadence: legs", 0x7c3aed),
    ("Dinner", "20:00 · Home", 0xdb2777),
    ("Read Frankenstein", "21:00 · ch. 5–6", 0x0d9488),
    ("Bed", "23:00 · Vesper", 0x4338ca),
];

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl App for Demo {
    fn dark(&self) -> Option<bool> {
        self.dark
    }

    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        let ctx = root.ctx().clone();
        let screen = ctx.content_rect();
        let sidebar = Rect::from_min_max(screen.min + Vec2::splat(GAP), Pos2::new(screen.min.x + 220.0, screen.max.y - GAP));
        let content = Rect::from_min_max(Pos2::new(sidebar.max.x + GAP, screen.min.y), screen.max);

        // The content: plain background, solid cards with coloured events. It scrolls under the bar.
        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg)).show(root, |ui| {
            let mut area = ui.new_child(egui::UiBuilder::new().max_rect(content.shrink2(Vec2::new(0.0, 0.0))));
            egui::ScrollArea::vertical()
                .vertical_scroll_offset(self.scroll)
                .show(&mut area, |ui| {
                    ui.add_space(86.0);
                    for (title, detail, colour) in EVENTS {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width() - GAP, 92.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(rect, CornerRadius::same(OUTER_RADIUS as u8), th.surface);
                        let inner = rect.shrink(CARD_PADDING);
                        p.rect_filled(inner, CornerRadius::same(nested(OUTER_RADIUS, CARD_PADDING) as u8), hex(colour));
                        p.text(inner.left_top() + Vec2::new(14.0, 14.0), Align2::LEFT_TOP, title, FontId::proportional(19.0), Color32::WHITE);
                        p.text(inner.left_top() + Vec2::new(14.0, 42.0), Align2::LEFT_TOP, detail, FontId::proportional(14.0), Color32::from_white_alpha(230));
                        ui.add_space(GAP);
                    }
                    ui.add_space(90.0);
                });
        });

        // The chrome: glass over the content.
        egui::Area::new(egui::Id::NULL.with("chrome")).order(Order::Foreground).fade_in(false).fixed_pos(Pos2::ZERO).show(&ctx, |ui| {
            let p = ui.painter();
            let bar = Rect::from_min_max(Pos2::new(content.min.x, screen.min.y + GAP), Pos2::new(content.max.x - GAP, screen.min.y + GAP + 64.0));
            glass::pane(p, bar, OUTER_RADIUS, th, Weight::Regular, true);
            p.text(bar.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, "Monday, 9 November", FontId::proportional(22.0), th.text);

            glass::pane(p, sidebar, OUTER_RADIUS, th, Weight::Regular, true);
            p.text(sidebar.left_top() + Vec2::new(18.0, 22.0), Align2::LEFT_TOP, "Quire", FontId::proportional(22.0), th.text);
            for (i, name) in ["Today", "Week", "Notes", "Water", "Money"].into_iter().enumerate() {
                let row = Rect::from_min_size(sidebar.left_top() + Vec2::new(10.0, 70.0 + i as f32 * 44.0), Vec2::new(sidebar.width() - 20.0, 38.0));
                if i == 0 {
                    p.rect_filled(row, CornerRadius::same(nested(OUTER_RADIUS, 10.0) as u8), th.accent_soft);
                }
                p.text(row.left_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(16.0), th.text);
            }

            let dock = Rect::from_center_size(Pos2::new(content.center().x, screen.max.y - GAP - 30.0), Vec2::new(300.0, 60.0));
            glass::pane(p, dock, 30.0, th, Weight::Light, true);
            for (i, name) in ["+ Task", "+ Event", "Today"].into_iter().enumerate() {
                p.text(Pos2::new(dock.min.x + 55.0 + i as f32 * 95.0, dock.center().y), Align2::CENTER_CENTER, name, FontId::proportional(16.0), th.text);
            }
        });
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let mut demo = Demo {
        dark: flag("--dark").map(|_| true).or(Some(false)),
        scroll: flag("--scroll").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(0.0),
    };
    match flag("--screenshot").and_then(|i| args.get(i + 1)) {
        Some(path) => quezka_kit::screenshot(&mut demo, [1000.0, 640.0], 1.0, 20, path),
        None => quezka_kit::run("Glass demo", demo),
    }
}
