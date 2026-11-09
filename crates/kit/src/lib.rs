//! quezka-kit: the Quezka suite's shared look in Rust (the rewrite of the Python `quezka_ui` and the phone themes).
//!
//! - `theme`: the suite's colours and shape (radii that nest, one gap).
//! - `glass`: liquid glass with a real backdrop blur, composited on the GPU.
//! - `fonts`, `style`: Manrope, and egui's own widgets in the suite's colours and sizes.
//! - `widgets`: the suite's controls (buttons, switch, segmented, cards, sidebar, settings rows).
//! - `runner`: the window loop, and offscreen screenshots for checks.

pub mod fonts;
pub mod glass;
pub mod icons;
mod symbols;
pub mod runner;
pub mod style;
pub mod theme;
pub mod tokens;
pub mod typing;
pub mod widgets;
#[cfg(target_os = "android")]
pub mod android;

#[cfg(not(target_os = "android"))]
pub use runner::run;
#[cfg(target_os = "android")]
pub use runner::run_android;
pub use runner::{screenshot, App};

/// A line for the developer: logcat (tag "quezka") on Android, stderr elsewhere.
pub fn log(line: &str) {
    #[cfg(target_os = "android")]
    {
        #[link(name = "log")]
        extern "C" {
            fn __android_log_write(priority: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
        }
        let tag = c"quezka";
        if let Ok(text) = std::ffi::CString::new(line) {
            // SAFETY: both strings are valid, NUL-terminated and live for the call.
            unsafe { __android_log_write(4, tag.as_ptr(), text.as_ptr()) };
        }
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("{line}");
}
pub use theme::Tokens;

/// The part of the screen free of system bars, notches and the keyboard (all zero on the desktop). Glass may run
/// under the bars; words and buttons stay inside this.
pub fn safe_area(ctx: &egui::Context) -> egui::Margin {
    ctx.data(|d| d.get_temp(egui::Id::NULL.with("quezka-safe-area"))).unwrap_or_default()
}
