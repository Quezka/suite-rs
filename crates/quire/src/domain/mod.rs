//! Quire's entities and rules: pure Rust, nothing about storage, screens or the network. A line-for-line port of the
//! Python `quire/domain` (and the phone's `domain/*.kt`); both must keep giving the same answers.

pub mod errors;
pub mod model;
pub mod payroll;

pub use errors::ValidationError;
