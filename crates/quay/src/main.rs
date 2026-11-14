#![cfg_attr(windows, windows_subsystem = "windows")]
//! quay [--demo] [--autostart] [--screenshot file.png [--dark] [--width W --height H]]

use std::sync::Arc;

use image::ImageEncoder;

use quay::application::service::SuiteService;
use quay::infrastructure::{autostart, demo, github, platforms};
use quay::presentation::window;

struct Shown {
    window: window::QuayWindow,
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
    // Packaging: the app's icon as a PNG (Linux) or ICO (Windows setup), from the bundled SVG.
    if let Some(path) = value("--write-icon") {
        return write_icon(&path);
    }
    if flag("--version").is_some() {
        println!("quay {}", quay::VERSION);
        return Ok(());
    }
    let service = if flag("--demo").is_some() {
        SuiteService::new(Arc::new(demo::DemoFeed), Arc::new(demo::DemoPlatform::default()), None)
    } else {
        SuiteService::new(Arc::new(github::GitHubFeed::new(quay::VERSION)), platforms::for_this_computer(), autostart::for_this_computer())
    };
    let quiet = flag("--autostart").is_some();
    let mut shown = Shown { window: window::QuayWindow::new(service, quay::infrastructure::icons::icon, quiet), dark: flag("--dark").map(|_| true) };
    if let Some(path) = value("--screenshot") {
        shown.window.settle();
        let w: f32 = value("--width").and_then(|v| v.parse().ok()).unwrap_or(820.0);
        let h: f32 = value("--height").and_then(|v| v.parse().ok()).unwrap_or(720.0);
        if shown.dark.is_none() {
            shown.dark = Some(false);
        }
        return quezka_kit::screenshot(&mut shown, [w, h], 1.0, 20, &path);
    }
    let icon = quay::infrastructure::icons::icon("quay")
        .and_then(|svg| quezka_kit::icons::render_svg(svg, 128))
        .map(|image| (image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect::<Vec<u8>>(), 128));
    let options = quezka_kit::Options { title: "Quay".into(), app_id: "io.github.quezka.Quay".into(), icon, start_hidden: quiet };
    quezka_kit::run_with(options, shown)
}

fn write_icon(path: &str) -> Result<(), String> {
    let svg = quay::infrastructure::icons::icon("quay").ok_or("no icon")?;
    let image = quezka_kit::icons::render_svg(svg, 256).ok_or("the icon doesn't render")?;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, 256, 256, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    let bytes = if path.ends_with(".ico") {
        // An ICO holding one 256 px PNG: Windows Vista and later read it.
        let mut ico = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&png);
        ico
    } else {
        png
    };
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
