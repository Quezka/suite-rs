//! Use cases and the ports they need (port of `quire/application`). Knows the domain; knows nothing of SQLite, the
//! network or screens: those are behind the traits in `ports`.

pub mod bus;
pub mod digest;
pub mod errors;
pub mod focus;
pub mod ideas;
pub mod medicine;
pub mod notes;
pub mod planner;
pub mod ports;
pub mod quiet;
pub mod reminders;
pub mod school;
pub mod sleep;
pub mod sync;
pub mod tasks;
pub mod timetable;
pub mod water;
pub mod work;
