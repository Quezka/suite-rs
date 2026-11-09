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
pub mod widgets;

pub use runner::{run, screenshot, App};
pub use theme::Tokens;
