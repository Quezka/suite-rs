//! loupe [--screenshot file.png [--dark]]

use loupe::application::LoupeService;
use loupe::infrastructure::{LinuxSystem, Udp};
use loupe::presentation::window::LoupeWindow;

struct Shown {
    window: LoupeWindow,
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

impl Drop for Shown {
    fn drop(&mut self) {
        self.window.shutdown();
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let value = |name: &str| flag(name).and_then(|i| args.get(i + 1)).cloned();
    if flag("--version").is_some() {
        println!("loupe {}", loupe::VERSION);
        return Ok(());
    }
    let service = LoupeService::new(Box::new(LinuxSystem::default()), Box::new(Udp::new()));
    let mut shown = Shown { window: LoupeWindow::new(service), dark: flag("--dark").map(|_| true) };
    if let Some(path) = value("--screenshot") {
        if shown.dark.is_none() {
            shown.dark = Some(false);
        }
        return quezka_kit::screenshot(&mut shown, [720.0, 860.0], 1.0, 20, &path);
    }
    let options = quezka_kit::Options { title: "Loupe".into(), app_id: "io.github.quezka.Loupe".into(), icon: None, start_hidden: false };
    quezka_kit::run_with(options, shown)
}
