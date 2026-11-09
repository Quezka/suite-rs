//! Quire in Rust. Clean Architecture, as in the Python app: `domain` (pure rules, no I/O) ← `application` (use cases
//! and ports) ← `infrastructure` and `presentation`. `tests/architecture.rs` keeps the arrows pointing inwards.

pub mod domain;
