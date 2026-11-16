//! quire-rs [--demo] [--page today|ideas] [--screenshot f.png [--dark] [--width W --height H]] [--db path]
//! beside python quire until parity; same db file

use quire::bootstrap::{build_services, Overrides};
use quire::presentation::window::{Page, QuireWindow};

struct Shown {
    window: QuireWindow,
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
        println!("quire-rs {}", quire::bootstrap::VERSION);
        return Ok(());
    }
    let chosen = std::env::var("QUIRE_LANG").unwrap_or_default();
    quezka_kit::i18n::init("QUIRE_LANG", &chosen);
    // demo = copy of python demo db (tests/fixtures) in tmp: never touches real data
    let db = if flag("--demo").is_some() {
        let path = std::env::temp_dir().join("quire-rs-demo.db");
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/python.db"), &path).map_err(|e| e.to_string())?;
        path
    } else {
        value("--db").map(Into::into).unwrap_or_else(|| quire::infrastructure::paths::data_dir().join("quire.db"))
    };
    let (services, _db) = build_services(&db, Overrides::default()).map_err(|e| e.to_string())?;
    let mut window = QuireWindow::new(services);
    if value("--page").as_deref() == Some("ideas") {
        window.page = Page::Ideas;
    }
    let mut app = Shown { window, dark: flag("--dark").map(|_| true) };
    match value("--screenshot") {
        Some(path) => {
            let size = [value("--width").and_then(|v| v.parse().ok()).unwrap_or(1366.0), value("--height").and_then(|v| v.parse().ok()).unwrap_or(720.0)];
            quezka_kit::screenshot(&mut app, size, 1.0, 20, &path)
        }
        None => {
            let r = quezka_kit::run("Quire", app);
            r
        }
    }
}
