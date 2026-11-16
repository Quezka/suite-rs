#![cfg_attr(windows, windows_subsystem = "windows")]
//! tidings [--demo] [--screenshot file.png [--dark] [--tab stories|blindspots|markets|sources] [--width W --height H]]

use std::sync::Arc;

use tidings::application::News;
use tidings::infrastructure::{demo, FileStore, WebFeeds};
use tidings::presentation::window::TidingsWindow;

struct Shown {
    window: TidingsWindow,
    dark: Option<bool>,
}

impl quezka_kit::App for Shown {
    fn ui(&mut self, ui: &mut egui::Ui, th: &quezka_kit::Tokens) {
        self.window.ui(ui, th);
    }
    fn dark(&self) -> Option<bool> {
        self.dark
    }
}

fn main() -> Result<(), String> {
    quezka_kit::i18n::init("TIDINGS_LANG", "");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let news = if flag("--demo") {
        News::new(Arc::new(demo::DemoFeeds), Box::new(demo::DemoStore::new()))
    } else {
        News::new(Arc::new(WebFeeds), Box::new(FileStore::new()))
    };
    let mut window = TidingsWindow::new(news, flag("--demo"));
    if let Some(tab) = value("--tab") {
        window.open(&tab);
    }
    let mut shown = Shown { window, dark: flag("--dark").then_some(true) };
    if let Some(path) = value("--screenshot") {
        shown.window.settle();
        if shown.dark.is_none() {
            shown.dark = Some(false);
        }
        let w: f32 = value("--width").and_then(|v| v.parse().ok()).unwrap_or(960.0);
        let h: f32 = value("--height").and_then(|v| v.parse().ok()).unwrap_or(720.0);
        return quezka_kit::screenshot(&mut shown, [w, h], 1.0, 24, &path);
    }
    let options = quezka_kit::Options {
        title: "Tidings".into(),
        app_id: "io.github.quezka.Tidings".into(),
        icon: None,
        ..Default::default()
    };
    quezka_kit::run_with(options, shown)
}
