//! Use cases and the ports they need (port of `quire/application`). Knows the domain; knows nothing of SQLite, the
//! network or screens: those are behind the traits in `ports`.

pub mod bus;
pub mod digest;
pub mod errors;
pub mod ideas;
pub mod notes;
pub mod planner;
pub mod ports;
pub mod tasks;
pub mod timetable;
