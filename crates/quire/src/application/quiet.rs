//! Do Not Disturb while a class, event or shift is on (port of `services/quiet.py`). Only undoes what it did itself.

use std::rc::Rc;

use chrono::{Duration, NaiveDateTime};

use super::errors::AppResult;
use super::planner::{ItemKind, PlannerService};
use super::ports::{Clock, KeyValueStore};

pub const CHECK_DAYS: i64 = 1;
const PREFIX: &str = "quiet.";

/// The computer's Do Not Disturb (GNOME's show-banners, Windows' focus assist...).
pub trait QuietSwitch {
    fn name(&self) -> String;
    fn available(&self) -> bool;
    fn set_quiet(&self, on: bool) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuietSettings {
    pub enabled: bool,
    pub classes: bool,
    pub events: bool,
    pub shifts: bool,
}

impl Default for QuietSettings {
    fn default() -> Self {
        Self { enabled: false, classes: true, events: true, shifts: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuietStatus {
    pub active: bool,
    pub until: Option<NaiveDateTime>,
    pub next_start: Option<NaiveDateTime>,
}

/// Sorted, with overlapping or touching periods joined (a shift over midnight is one).
pub fn merge_periods(mut periods: Vec<(NaiveDateTime, NaiveDateTime)>) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    periods.sort();
    let mut merged: Vec<(NaiveDateTime, NaiveDateTime)> = Vec::new();
    for (start, end) in periods {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

pub fn status_at(periods: &[(NaiveDateTime, NaiveDateTime)], now: NaiveDateTime) -> QuietStatus {
    let merged = merge_periods(periods.to_vec());
    if let Some((_, end)) = merged.iter().find(|(s, e)| *s <= now && now < *e) {
        return QuietStatus { active: true, until: Some(*end), next_start: None };
    }
    QuietStatus { active: false, until: None, next_start: merged.iter().map(|(s, _)| *s).filter(|s| *s > now).min() }
}

pub struct QuietService {
    pub planner: Rc<PlannerService>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub switch: Rc<dyn QuietSwitch>,
}

impl QuietService {
    pub fn settings(&self) -> QuietSettings {
        let d = QuietSettings::default();
        let flag = |n: &str, def: bool| self.settings.get(&format!("{PREFIX}{n}")).map_or(def, |v| v == "1");
        QuietSettings { enabled: flag("enabled", d.enabled), classes: flag("classes", d.classes), events: flag("events", d.events),
                        shifts: flag("shifts", d.shifts) }
    }

    pub fn save_settings(&self, s: QuietSettings) -> AppResult<bool> {
        for (n, v) in [("enabled", s.enabled), ("classes", s.classes), ("events", s.events), ("shifts", s.shifts)] {
            self.settings.set(&format!("{PREFIX}{n}"), Some(if v { "1" } else { "0" }))?;
        }
        self.tick()
    }

    pub fn available(&self) -> bool {
        self.switch.available()
    }

    pub fn method(&self) -> String {
        self.switch.name()
    }

    fn wanted(kind: ItemKind, s: &QuietSettings) -> bool {
        match kind {
            ItemKind::Class => s.classes,
            ItemKind::Event => s.events,
            ItemKind::Shift | ItemKind::WeeklyShift => s.shifts,
            ItemKind::Routine => false,
        }
    }

    pub fn periods(&self, now: NaiveDateTime, s: &QuietSettings) -> AppResult<Vec<(NaiveDateTime, NaiveDateTime)>> {
        let today = now.date();
        let mut found = Vec::new();
        for item in self.planner.items_between(today, today + Duration::days(CHECK_DAYS))? {
            if item.is_milestone() || item.is_all_day() || !Self::wanted(item.kind, s) {
                continue;
            }
            let midnight = item.day.and_hms_opt(0, 0, 0).unwrap();
            found.push((midnight + Duration::minutes(item.time.start as i64), midnight + Duration::minutes(item.time.end as i64)));
        }
        Ok(merge_periods(found))
    }

    pub fn status(&self) -> AppResult<QuietStatus> {
        let s = self.settings();
        if !s.enabled {
            return Ok(QuietStatus { active: false, until: None, next_start: None });
        }
        let now = self.clock.now();
        Ok(status_at(&self.periods(now, &s)?, now))
    }

    /// Bring the computer in line with the schedule; true while quiet.
    pub fn tick(&self) -> AppResult<bool> {
        let want = self.status()?.active && self.switch.available();
        let owned = self.settings.get(&format!("{PREFIX}applied")).as_deref() == Some("1");
        if want && !owned {
            // noted first: if switching fails halfway it's still ours to undo, and never switched on twice
            self.settings.set(&format!("{PREFIX}applied"), Some("1"))?;
            let _ = self.switch.set_quiet(true);
        } else if !want && owned {
            self.undo()?;
        }
        Ok(want)
    }

    /// Forgotten only once the switch says it worked: otherwise the next tick tries again.
    fn undo(&self) -> AppResult {
        if self.switch.set_quiet(false).is_ok() {
            self.settings.set(&format!("{PREFIX}applied"), None)?;
        }
        Ok(())
    }

    /// Give the notifications back (Quire closing, or it crashed while quiet).
    pub fn release(&self) -> AppResult {
        if self.settings.get(&format!("{PREFIX}applied")).as_deref() == Some("1") {
            self.undo()?;
        }
        Ok(())
    }
}
