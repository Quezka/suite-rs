//! Tidings: own Ground-News-like reader. layers: domain <- application <- infrastructure | presentation
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
