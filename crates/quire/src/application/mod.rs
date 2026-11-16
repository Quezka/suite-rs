//! Use cases and the ports they need (port of `quire/application`). Knows the domain; knows nothing of SQLite, the
//! network or screens: those are behind the traits in `ports`.

pub mod bus;
pub mod digest;
pub mod errors;
pub mod ideas;
pub mod medicine;
pub mod notes;
pub mod planner;
pub mod ports;
pub mod sleep;
pub mod tasks;
pub mod timetable;
pub mod water;
