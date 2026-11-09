//! Quay: installs and updates the Quezka suite from its GitHub releases (Linux .deb, Windows setup).
//! Clean Architecture: `domain` ← `application` ← `infrastructure` and `presentation`, wired in `main.rs`;
//! `tests/architecture.rs` keeps the arrows pointing inward.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
