//! tether [--background] [--demo] [--section N [--thread ADDR]] [--screenshot file.png [--dark] [--width W --height H]]

use std::sync::mpsc::channel;

use tether::infrastructure::{autostart_on_first_run, single_instance, Link, LinuxDesktop};
use tether::presentation::window::TetherWindow;

struct Shown {
    window: TetherWindow,
    dark: Option<bool>,
    /// tray (0.3): click = show window, menu = show | quit; tooltip = link status
    tray: Option<quezka_kit::tray::Tray>,
    tray_status: String,
}

impl quezka_kit::App for Shown {
    fn ui(&mut self, ui: &mut egui::Ui, th: &quezka_kit::Tokens) {
        if let Some(tray) = &self.tray {
            for e in tray.events() {
                match e {
                    quezka_kit::tray::TrayEvent::Show => {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    quezka_kit::tray::TrayEvent::Quit => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
            let status = self.window.status_line();
            if status != self.tray_status {
                tray.set_status(&status);
                self.tray_status = status;
            }
        }
        self.window.ui(ui, th);
    }
    fn dark(&self) -> Option<bool> {
        self.dark
    }
}

fn main() -> Result<(), String> {
    quezka_kit::i18n::init("TETHER_LANG", "");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let value = |name: &str| flag(name).and_then(|i| args.get(i + 1)).cloned();
    if flag("--version").is_some() {
        println!("tether {}", tether::VERSION);
        return Ok(());
    }
    let screenshot = value("--screenshot");
    // one per user: a second launch shows the running one's window and ends
    let requests = if screenshot.is_none() {
        match single_instance() {
            Ok(r) => Some(r),
            Err(()) => return Ok(()),
        }
    } else {
        None
    };
    let (tx, rx) = channel();
    if flag("--demo").is_some() {
        // made-up phone for screenshots: no network
        use tether::application::LinkEvent;
        use serde_json::json;
        let _ = tx.send(LinkEvent::Connected("Redmi Note 11 Pro+".into()));
        for m in [json!({"t": "battery", "level": 76, "charging": true}),
                  json!({"t": "notif", "key": "a", "app": "WhatsApp", "title": "Sam", "text": "Are we still on for 8?", "reply": true}),
                  json!({"t": "notif", "key": "b", "app": "Classeviva", "title": "New grade", "text": "Chemistry: 8"}),
                  json!({"t": "sms_list", "threads": [
                      {"addr": "+39 333 1", "name": "Mum", "messages": [
                          {"in": true, "body": "Dinner at 8?", "at": 1}, {"in": false, "body": "Yes, I'll bring bread", "at": 2},
                          {"in": true, "body": "Great, see you then. Don't forget the keys this time!", "at": 3}]},
                      {"addr": "+39 333 2", "name": "", "messages": [{"in": true, "body": "Your parcel arrives tomorrow", "at": 0}]}]})] {
            let _ = tx.send(LinkEvent::Message(m));
        }
    }
    let link = Link::start(tx);
    let mut window = TetherWindow::new(Box::new(link), Box::new(LinuxDesktop), rx);
    if let Some(r) = requests { window = window.with_show_requests(r); }
    if let Some(section) = value("--section").and_then(|v| v.parse().ok()) {
        window = window.open(section, value("--thread"));
    }
    let mut shown = Shown { window, dark: flag("--dark").map(|_| true), tray: None, tray_status: String::new() };
    if let Some(path) = screenshot {
        if shown.dark.is_none() { shown.dark = Some(false); }
        let w: f32 = value("--width").and_then(|v| v.parse().ok()).unwrap_or(1100.0);
        let h: f32 = value("--height").and_then(|v| v.parse().ok()).unwrap_or(720.0);
        return quezka_kit::screenshot(&mut shown, [w, h], 1.0, 20, &path);
    }
    autostart_on_first_run();
    shown.tray = quezka_kit::tray::spawn("io.github.quezka.Tether", "Tether", None,
        &tether::presentation::i18n::t("Show Tether"), &tether::presentation::i18n::t("Quit Tether"));
    let options = quezka_kit::Options { title: "Tether".into(), app_id: "io.github.quezka.Tether".into(), icon: None,
        start_hidden: flag("--background").is_some(), close_hides: true };
    quezka_kit::run_with(options, shown)
}
