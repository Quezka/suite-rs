//! wall clock, local time, whole seconds (= python `SystemClock`)

use chrono::{Local, NaiveDateTime, Timelike};

use crate::application::ports::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> NaiveDateTime {
        let now = Local::now().naive_local();
        now.with_nanosecond(0).unwrap_or(now)
    }
}
