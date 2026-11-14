//! Loupe: a laptop's camera on another computer (a normal webcam there), and the laptop as that computer's
//! second screen. Both over the home network. Clean Architecture: `domain` ← `application` ← `infrastructure`
//! and `presentation`, wired in `main.rs`; `tests/architecture.rs` keeps the arrows pointing inward.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
