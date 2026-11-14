//! Tether: the phone on the computer (notifications, SMS, calls, files, clipboard) over the home network,
//! end-to-end encrypted with a key made when pairing. Protocol: `docs/PROTOCOL.md` in the tether repo.
//! Clean Architecture: `domain` ← `application` ← `infrastructure` and `presentation`, wired in `main.rs`.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
