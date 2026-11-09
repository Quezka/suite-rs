//! quezka-kit: the Quezka suite's shared look in Rust (the rewrite of the Python `quezka_ui` and the phone themes).
//!
//! - `theme`: the suite's colours and shape (radii that nest, one gap).
//! - `glass`: liquid glass with a real backdrop blur, composited on the GPU.
//! - `runner`: the window loop, and offscreen screenshots for checks.

pub mod glass;
pub mod runner;
pub mod theme;

pub use runner::{run, screenshot, App};
pub use theme::Tokens;
