//! The suite's Settings window in Rust: a glass sidebar of sections floating over the window, cards of rows that
//! scroll under a glass header.
//!
//! cargo run --example settings_demo [-- --screenshot x.png [--dark] [--scroll 120] [--contrast]]

use egui::{Align2, Order, Pos2, Rect, Vec2};
use quezka_kit::fonts::Weight;
use quezka_kit::glass::{self, Weight as Glass};
use quezka_kit::theme::{GAP, OUTER_RADIUS};
use quezka_kit::widgets as w;
use quezka_kit::{App, Tokens};

struct Settings {
    dark: Option<bool>,
    contrast: bool,
    scroll: f32,
    section: usize,
    appearance: usize,
    glass: bool,
    see_through: bool,
    chime: bool,
    dnd: bool,
}

const SECTIONS: [(&str, &str); 5] = [("Look", "settings"), ("Reminders", "bell"), ("Sync", "refresh"), ("Startup", "play"), ("Your data", "history")];

impl App for Settings {
    fn dark(&self) -> Option<bool> {
        self.dark
    }
    fn high_contrast(&self) -> bool {
        self.contrast
    }

    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        let ctx = root.ctx().clone();
        let screen = ctx.content_rect();
        let sidebar = Rect::from_min_max(screen.min + Vec2::splat(GAP), Pos2::new(screen.min.x + 240.0, screen.max.y - GAP));
        let column = Rect::from_min_max(Pos2::new(sidebar.max.x + GAP, screen.min.y), Pos2::new(screen.max.x - GAP, screen.max.y));
        let header = Rect::from_min_size(Pos2::new(column.min.x, screen.min.y + GAP), Vec2::new(column.width(), 60.0));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg)).show(root, |ui| {
            let mut area = ui.new_child(egui::UiBuilder::new().max_rect(column));
            egui::ScrollArea::vertical().vertical_scroll_offset(self.scroll).show(&mut area, |ui| {
                ui.add_space(header.height() + 2.0 * GAP);
                w::settings_card(ui, th, Some("Appearance"), |ui| {
                    w::setting_row(ui, th, "Theme", Some("Follow the system, or always light or dark."), 300.0, |ui| {
                        if let Some(i) = w::segmented(ui, th, &["System", "Light", "Dark"], self.appearance) {
                            self.appearance = i;
                        }
                    });
                    w::setting_row(ui, th, "Glass look", Some("Menus, bars and headers blur what's under them."), 60.0, |ui| {
                        if w::switch(ui, th, self.glass) {
                            self.glass = !self.glass;
                        }
                    });
                    w::setting_row(ui, th, "See-through windows", Some("The desktop shows through, blurred."), 60.0, |ui| {
                        if w::switch(ui, th, self.see_through) {
                            self.see_through = !self.see_through;
                        }
                    });
                });
                w::settings_card(ui, th, Some("Reminders"), |ui| {
                    w::setting_row(ui, th, "Play Quire's chime with notifications", None, 60.0, |ui| {
                        if w::switch(ui, th, self.chime) {
                            self.chime = !self.chime;
                        }
                    });
                    w::setting_row(ui, th, "Silence notifications while things are on", Some("Turns Do Not Disturb on for GNOME from the start to the end of each one, while Quire is open, and puts it back afterwards."), 60.0, |ui| {
                        if w::switch(ui, th, self.dnd) {
                            self.dnd = !self.dnd;
                        }
                    });
                });
                w::settings_card(ui, th, Some("Language"), |ui| {
                    w::setting_row(ui, th, "Language", Some("Restart to see the whole app in the new language."), 360.0, |ui| {
                        w::segmented(ui, th, &["System", "English", "Italiano", "Русский"], 0);
                    });
                });
                ui.add_space(GAP);
            });
        });

        egui::Area::new(egui::Id::NULL.with("chrome")).order(Order::Foreground).fade_in(false).fixed_pos(Pos2::ZERO).show(&ctx, |ui| {
            glass::pane(ui.painter(), sidebar, OUTER_RADIUS, th, Glass::Regular, true);
            ui.painter().text(sidebar.left_top() + Vec2::new(18.0, 20.0), Align2::LEFT_TOP, "Settings", Weight::Bold.font(26.0), th.text);
            let mut list = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(sidebar.min + Vec2::new(10.0, 70.0), sidebar.max - Vec2::splat(10.0))));
            for (i, (name, glyph)) in SECTIONS.into_iter().enumerate() {
                if w::sidebar_item(&mut list, th, glyph, name, i == self.section) {
                    self.section = i;
                }
            }
            if self.scroll > 0.0 {
                glass::pane(ui.painter(), header, OUTER_RADIUS, th, Glass::Regular, true);
            }
            ui.painter().text(header.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, SECTIONS[self.section].0, Weight::Bold.font(22.0), th.text);
        });
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let mut app = Settings {
        dark: Some(flag("--dark").is_some()),
        contrast: flag("--contrast").is_some(),
        scroll: flag("--scroll").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(0.0),
        section: 0,
        appearance: 0,
        glass: true,
        see_through: true,
        chime: true,
        dnd: false,
    };
    match flag("--screenshot").and_then(|i| args.get(i + 1)) {
        Some(path) => quezka_kit::screenshot(&mut app, [900.0, 560.0], 1.0, 20, path),
        None => quezka_kit::run("Settings", app),
    }
}
