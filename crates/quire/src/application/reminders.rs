//! "Remind me before my next thing starts" (port of `services/reminders.py`). The UI calls `due()` every so often;
//! each start is announced once per run of the app.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use chrono::{Duration, NaiveDateTime};

use super::errors::AppResult;
use super::planner::{AgendaItem, ItemKind, PlannerService};
use super::ports::{Clock, KeyValueStore};

/// How long after something starts a reminder still makes sense; older is skipped, not announced late.
pub const GRACE_MINUTES: i64 = 2;
pub const CHOICES: [Option<i32>; 7] = [None, Some(0), Some(5), Some(10), Some(15), Some(30), Some(60)];
const PREFIX: &str = "reminders.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReminderSettings {
    pub minutes_before: Option<i32>,
    pub events: bool,
    pub classes: bool,
    pub shifts: bool,
}

impl Default for ReminderSettings {
    fn default() -> Self {
        Self { minutes_before: Some(10), events: true, classes: false, shifts: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reminder {
    pub item: AgendaItem,
    pub starts: NaiveDateTime,
    /// 0 when it's starting now.
    pub minutes_left: i64,
}

pub struct ReminderService {
    pub planner: Rc<PlannerService>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub sent: RefCell<HashSet<(ItemKind, Option<i64>, NaiveDateTime)>>,
}

impl ReminderService {
    pub fn settings(&self) -> ReminderSettings {
        let d = ReminderSettings::default();
        let before = match self.settings.get(&format!("{PREFIX}minutes")) {
            None => d.minutes_before,
            Some(m) => m.parse().ok().filter(|_| m.bytes().all(|b| b.is_ascii_digit())),
        };
        let flag = |n: &str, def: bool| self.settings.get(&format!("{PREFIX}{n}")).map_or(def, |v| v == "1");
        ReminderSettings { minutes_before: before, events: flag("events", d.events), classes: flag("classes", d.classes),
                           shifts: flag("shifts", d.shifts) }
    }

    pub fn save_settings(&self, s: ReminderSettings) -> AppResult {
        self.settings.set(&format!("{PREFIX}minutes"), Some(&s.minutes_before.map_or("off".into(), |m| m.to_string())))?;
        for (n, v) in [("events", s.events), ("classes", s.classes), ("shifts", s.shifts)] {
            self.settings.set(&format!("{PREFIX}{n}"), Some(if v { "1" } else { "0" }))?;
        }
        Ok(())
    }

    fn wanted(kind: ItemKind, s: &ReminderSettings) -> bool {
        match kind {
            ItemKind::Class => s.classes,
            ItemKind::Event | ItemKind::Routine => s.events,
            _ => s.shifts,
        }
    }

    /// Things starting within the reminder time that haven't been announced yet.
    pub fn due(&self) -> AppResult<Vec<Reminder>> {
        let s = self.settings();
        let Some(before) = s.minutes_before else { return Ok(Vec::new()) };
        let now = self.clock.now();
        let horizon = now + Duration::minutes(before as i64);
        let earliest = now - Duration::minutes(GRACE_MINUTES);
        let mut found = Vec::new();
        for item in self.planner.items_between(earliest.date(), horizon.date())? {
            if item.is_all_day() || !Self::wanted(item.kind, &s) {
                continue; // an all-day event has no moment to be reminded of
            }
            let (day, minute) = item.origin.unwrap_or((item.day, item.time.start));
            if (day, minute) != (item.day, item.time.start) {
                continue; // the after-midnight part of something that started yesterday
            }
            let starts = day.and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(minute as i64);
            let key = (item.kind, item.ref_id, starts);
            if earliest < starts && starts <= horizon && self.sent.borrow_mut().insert(key) {
                let secs = (starts - now).num_seconds();
                let left = (secs + 59).div_euclid(60).max(0); // rounded up
                found.push(Reminder { item, starts, minutes_left: left });
            }
        }
        found.sort_by_key(|r| r.starts);
        Ok(found)
    }
}
