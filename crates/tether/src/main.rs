//! tether [--screenshot file.png [--dark]]

use std::sync::mpsc::channel;

use tether::infrastructure::{Link, LinuxDesktop};
use tether::presentation::window::TetherWindow;

struct Shown {
    window: TetherWindow,
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let value = |name: &str| flag(name).and_then(|i| args.get(i + 1)).cloned();
    if flag("--version").is_some() {
        println!("tether {}", tether::VERSION);
        return Ok(());
    }
    let (tx, rx) = channel();
    let link = Link::start(tx);
    let mut shown = Shown { window: TetherWindow::new(Box::new(link), Box::new(LinuxDesktop), rx), dark: flag("--dark").map(|_| true) };
    if let Some(path) = value("--screenshot") {
        if shown.dark.is_none() { shown.dark = Some(false); }
        return quezka_kit::screenshot(&mut shown, [820.0, 760.0], 1.0, 20, &path);
    }
    let options = quezka_kit::Options { title: "Tether".into(), app_id: "io.github.quezka.Tether".into(), icon: None,
        start_hidden: flag("--background").is_some() };
    quezka_kit::run_with(options, shown)
}
