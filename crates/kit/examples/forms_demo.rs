//! kit forms check: fields, date + time picker, dialog, menu
//! cargo run --example forms_demo [-- --screenshot x.png [--dark] [--dialog]]

use chrono::NaiveDate;
use quezka_kit::forms as f;
use quezka_kit::theme::Tokens;
use quezka_kit::widgets::{self as w, Kind};

struct Demo {
    dark: Option<bool>,
    dialog: bool,
    title: String,
    notes: String,
    day: NaiveDate,
    minutes: i32,
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

impl quezka_kit::App for Demo {
    fn dark(&self) -> Option<bool> {
        self.dark
    }
    fn ui(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        egui::Frame::new().fill(th.bg).inner_margin(24).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(2, |c| {
                w::card(&mut c[0], th, |ui| {
                    w::headline(ui, th, "New task");
                    ui.add_space(8.0);
                    f::field(ui, th, "Title", &mut self.title, "What needs doing?");
                    f::area(ui, th, "Notes", &mut self.notes, "Anything to remember", 3);
                    ui.label(w::text("At", 14.0, th.muted, quezka_kit::fonts::Weight::SemiBold));
                    f::time_picker(ui, th, &mut self.minutes, 5);
                    let more = ui.add(egui::Button::new("More…"));
                    f::menu(th, &more, |ui| {
                        f::menu_item(ui, th, Some("edit"), "Edit", false);
                        f::menu_item(ui, th, Some("trash"), "Delete", true);
                    });
                    if f::dialog_buttons(ui, th, &[("Cancel", Kind::Text), ("Save", Kind::Filled)]) == Some(1) {
                        self.dialog = true;
                    }
                });
                w::card(&mut c[1], th, |ui| {
                    w::headline(ui, th, "Due");
                    let weekdays = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map(String::from);
                    f::date_picker(ui, th, ui.id().with("due"), &mut self.day, NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
                        &|m| MONTHS[m as usize - 1].to_string(), &weekdays);
                });
            });
        });
        if self.dialog {
            let out = f::dialog(ui.ctx(), th, ui.id().with("confirm"), "Delete this task?", |ui| {
                ui.label(w::text("It goes away on every device. You can undo for a few seconds.", 16.0, th.muted, quezka_kit::fonts::Weight::Regular));
                if let Some(i) = f::dialog_buttons(ui, th, &[("Keep", Kind::Text), ("Delete", Kind::Danger)]) {
                    let _ = i;
                    self.dialog = false;
                }
            });
            if out.dismissed {
                self.dialog = false;
            }
        }
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let mut app = Demo {
        dark: Some(flag("--dark").is_some()), dialog: flag("--dialog").is_some(), title: "Read chapter 3".into(),
        notes: String::new(), day: NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(), minutes: 17 * 60 + 30,
    };
    match flag("--screenshot").and_then(|i| args.get(i + 1)) {
        Some(path) => quezka_kit::screenshot(&mut app, [1000.0, 640.0], 1.0, 20, path),
        None => quezka_kit::run("Forms", app),
    }
}
