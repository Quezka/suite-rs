//! Water: the goal from height and weight, what you drank, a nudge when behind (port of `services/water.py`).
//! The profile stays on each device (settings `water.*`).

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Duration, NaiveDate, Timelike};

use super::bus::{ChangeBus, Topic};
use super::errors::AppResult;
use super::ports::{Clock, KeyValueStore, WaterRepository};
use crate::domain::water::{check_manual_goal, expected_ml, reminder_due, water_goal_ml, water_streak, WaterLog,
                           DEFAULT_END, DEFAULT_GLASS, DEFAULT_START};
use crate::domain::ValidationError;

pub const STREAK_DAYS: i64 = 120;
const PREFIX: &str = "water.";

#[derive(Clone, Debug, PartialEq)]
pub struct WaterProfile {
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub glass_ml: i64,
    pub remind: bool,
    pub start: i32,
    pub end: i32,
    pub manual_goal: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaterOverview {
    pub profile: WaterProfile,
    pub goal: Option<i64>,
    pub drunk: i64,
    pub expected: i64,
    /// (id, minute, ml) of `day`, oldest first.
    pub entries: Vec<(i64, i32, i64)>,
    /// The last 7 days, oldest first.
    pub week: Vec<(NaiveDate, i64)>,
    pub streak: usize,
    pub average: i64,
    pub day: NaiveDate,
    pub on_goal: usize,
}

pub struct WaterService {
    pub logs: Rc<dyn WaterRepository>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
    /// Minute of today the last reminder showed; forgotten at midnight.
    pub last_reminder: Cell<Option<i32>>,
}

impl WaterService {
    fn number(&self, name: &str) -> Option<f64> {
        self.settings.get(&format!("{PREFIX}{name}")).and_then(|v| v.trim().parse().ok())
    }

    pub fn profile(&self) -> WaterProfile {
        let whole = |n: &str, d: i64| self.number(n).map_or(d, |v| v as i64);
        WaterProfile {
            height_cm: self.number("height"), weight_kg: self.number("weight"), glass_ml: whole("glass", DEFAULT_GLASS),
            remind: self.settings.get(&format!("{PREFIX}remind")).as_deref() != Some("0"),
            start: whole("start", DEFAULT_START as i64) as i32, end: whole("end", DEFAULT_END as i64) as i32,
            manual_goal: Some(whole("manual_goal", 0)).filter(|g| *g != 0),
        }
    }

    pub fn save_profile(&self, p: &WaterProfile) -> AppResult<i64> {
        let body = water_goal_ml(p.height_cm.unwrap_or(0.0), p.weight_kg.unwrap_or(0.0))?;
        let goal = match p.manual_goal { Some(g) if g != 0 => check_manual_goal(g)?, _ => body };
        if !(50..=1000).contains(&p.glass_ml) {
            return Err(ValidationError("Enter an amount between 50 and 1000 ml.").into());
        }
        if !(0 <= p.start && p.start < p.end && p.end <= 24 * 60) {
            return Err(ValidationError("Reminders must start before they end.").into());
        }
        let py = |v: f64| if v.fract() == 0.0 { format!("{v:.1}") } else { v.to_string() }; // str(float) like Python
        for (name, value) in [("height", py(p.height_cm.unwrap_or(0.0))), ("weight", py(p.weight_kg.unwrap_or(0.0))),
                              ("glass", p.glass_ml.to_string()), ("start", p.start.to_string()), ("end", p.end.to_string())] {
            self.settings.set(&format!("{PREFIX}{name}"), Some(&value))?;
        }
        self.settings.set(&format!("{PREFIX}manual_goal"), Some(&p.manual_goal.unwrap_or(0).to_string()))?;
        self.settings.set(&format!("{PREFIX}remind"), Some(if p.remind { "1" } else { "0" }))?;
        self.bus.publish(Topic::Water);
        Ok(goal)
    }

    /// Log a drink now or, for a day you forgot, on that day at the same time of day.
    pub fn add(&self, ml: i64, day: Option<NaiveDate>) -> AppResult {
        let now = self.clock.now();
        let day = day.unwrap_or(now.date());
        if day > now.date() {
            return Err(ValidationError("You can't log a day that hasn't come yet.").into());
        }
        let log = WaterLog { day, at: (now.hour() * 60 + now.minute()) as i32, ml, id: None };
        log.validate()?;
        self.logs.add(&log)?;
        self.bus.publish(Topic::Water);
        Ok(())
    }

    pub fn undo_last(&self, day: Option<NaiveDate>) -> AppResult {
        let day = day.unwrap_or(self.clock.today());
        if let Some(last) = self.logs.between(day, day)?.last() {
            self.logs.delete(last.id.unwrap_or_default())?;
            self.bus.publish(Topic::Water);
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> AppResult {
        self.logs.delete(id)?;
        self.bus.publish(Topic::Water);
        Ok(())
    }

    /// `day` picks whose count and entries to show; the goal, week and streak stay as of today.
    pub fn overview(&self, day: Option<NaiveDate>) -> AppResult<WaterOverview> {
        let today = self.clock.today();
        let day = day.unwrap_or(today);
        let profile = self.profile();
        let goal = profile.manual_goal.or_else(|| match (profile.height_cm, profile.weight_kg) {
            (Some(h), Some(w)) if h != 0.0 && w != 0.0 => water_goal_ml(h, w).ok(),
            _ => None,
        });
        let logs = self.logs.between(today - Duration::days(STREAK_DAYS), today)?;
        let mine = self.logs.between(day, day)?;
        let drunk: i64 = mine.iter().map(|l| l.ml).sum();
        let now = self.clock.now();
        let expected = match goal {
            Some(g) if day == today => expected_ml(g, (now.hour() * 60 + now.minute()) as i32, profile.start, profile.end),
            _ => 0,
        };
        let mut per_day: HashMap<NaiveDate, i64> = HashMap::new();
        for l in &logs {
            *per_day.entry(l.day).or_default() += l.ml;
        }
        let week: Vec<(NaiveDate, i64)> = (0..7).rev().map(|i| { let d = today - Duration::days(i); (d, *per_day.get(&d).unwrap_or(&0)) }).collect();
        let days: Vec<i64> = (0..=STREAK_DAYS).map(|i| *per_day.get(&(today - Duration::days(i))).unwrap_or(&0)).collect();
        Ok(WaterOverview {
            streak: goal.map_or(0, |g| water_streak(&days, g)),
            average: days[1..8].iter().sum::<i64>() / 7,
            on_goal: goal.map_or(0, |g| week.iter().filter(|(_, ml)| *ml >= g).count()),
            entries: mine.iter().map(|l| (l.id.unwrap_or_default(), l.at, l.ml)).collect(),
            profile, goal, drunk, expected, week, day,
        })
    }

    /// A line to show now if you're behind on water ("drunk/goal"), else None. Call about every minute.
    pub fn reminder(&self) -> AppResult<Option<String>> {
        let o = self.overview(None)?;
        let Some(goal) = o.goal else { return Ok(None) };
        if !o.profile.remind {
            return Ok(None);
        }
        let now = self.clock.now();
        let minute = (now.hour() * 60 + now.minute()) as i32;
        if self.last_reminder.get().is_some_and(|l| l > minute) {
            self.last_reminder.set(None); // a new day
        }
        if !reminder_due(goal, o.drunk, minute, self.last_reminder.get(), o.profile.start, o.profile.end) {
            return Ok(None);
        }
        self.last_reminder.set(Some(minute));
        Ok(Some(format!("{}/{goal}", o.drunk)))
    }
}
