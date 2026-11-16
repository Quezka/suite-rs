//! The sleep log and the sleep goal (port of `services/sleep.py`).

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};

use super::bus::{ChangeBus, Topic};
use super::errors::AppResult;
use super::ports::{Clock, FocusLogRepository, KeyValueStore, SleepRepository};
use crate::domain::sleep::{bedtime_for, minutes_until, regularity, should_wind_down, sleep_stats, SleepLog,
                           DEFAULT_GOAL_HOURS, DEFAULT_WAKE, ENOUGH_RATIO};
use crate::domain::ValidationError;

pub const NIGHTS: i64 = 14;
pub const MIN_PER_GROUP: usize = 3; // nights needed on each side before comparing focus
const PREFIX: &str = "sleep.";
const WIND_DOWN_LEAD: i32 = 30;

#[derive(Clone, Debug)]
pub struct SleepOverview {
    pub goal_hours: f64,
    pub wake_goal: i32,
    pub bedtime_goal: i32,
    pub nights: Vec<SleepLog>,
    pub average_minutes: i32,
    pub average_bedtime: Option<i32>,
    pub average_wake: Option<i32>,
    pub average_quality: Option<f64>,
    pub enough: usize,
    pub shortfall_minutes: i32,
    pub bedtime_spread: i32,
    pub regularity: &'static str,
    pub streak: usize,
    pub last_week_average: Option<i32>,
    pub focus_after_enough: Option<i64>,
    pub focus_after_short: Option<i64>,
    pub asleep_since: Option<i32>,
    pub days: Vec<(NaiveDate, Option<SleepLog>)>,
    pub last_night: Option<SleepLog>,
    pub bedtime_in: i32,
}

pub struct SleepService {
    pub logs: Rc<dyn SleepRepository>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
    pub focus: Option<Rc<dyn FocusLogRepository>>,
    pub reminded_on: Cell<Option<NaiveDate>>,
}

fn minute_of(t: NaiveDateTime) -> i32 {
    (t.hour() * 60 + t.minute()) as i32
}

impl SleepService {
    fn key(name: &str) -> String {
        format!("{PREFIX}{name}")
    }

    /// (hours of sleep wanted, minute of the day to wake up).
    pub fn goal(&self) -> (f64, i32) {
        let hours = self.settings.get(&Self::key("hours")).and_then(|h| h.parse::<f64>().ok());
        let wake = self.settings.get(&Self::key("wake")).and_then(|w| w.parse::<i32>().ok());
        let (h, w) = match (self.settings.get(&Self::key("hours")).is_some() && hours.is_none(),
                            self.settings.get(&Self::key("wake")).is_some() && wake.is_none()) {
            (false, false) => (hours.unwrap_or(DEFAULT_GOAL_HOURS), wake.unwrap_or(DEFAULT_WAKE)),
            _ => (DEFAULT_GOAL_HOURS, DEFAULT_WAKE), // a broken value: both back to the defaults, as Python
        };
        (h.clamp(3.0, 14.0), w.rem_euclid(24 * 60))
    }

    pub fn save_goal(&self, hours: f64, wake: i32) -> AppResult {
        let h = hours.clamp(3.0, 14.0);
        self.settings.set(&Self::key("hours"), Some(&if h.fract() == 0.0 { format!("{h:.1}") } else { h.to_string() }))?;
        self.settings.set(&Self::key("wake"), Some(&wake.rem_euclid(24 * 60).to_string()))?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    pub fn asleep_since(&self) -> Option<NaiveDateTime> {
        self.settings.get(&Self::key("pending")).and_then(|v| NaiveDateTime::parse_from_str(&v, "%Y-%m-%dT%H:%M").ok())
    }

    /// One tap: "going to sleep" now.
    pub fn go_to_sleep(&self) -> AppResult {
        self.settings.set(&Self::key("pending"), Some(&self.clock.now().format("%Y-%m-%dT%H:%M").to_string()))?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    pub fn cancel_going_to_sleep(&self) -> AppResult {
        self.settings.set(&Self::key("pending"), None)?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    /// Close the night started with `go_to_sleep`, waking now.
    pub fn wake_up(&self) -> AppResult<SleepLog> {
        let since = self.asleep_since().ok_or(ValidationError("Say when you go to sleep first."))?;
        let now = self.clock.now();
        let slept = now - since;
        if !(Duration::minutes(30) <= slept && slept <= Duration::hours(18)) {
            return Err(ValidationError("A night's sleep is between 30 minutes and 18 hours.").into());
        }
        let log = SleepLog { day: now.date(), asleep: minute_of(since), woke: minute_of(now), quality: None, note: String::new() };
        log.validate()?;
        self.logs.put(&log)?;
        self.settings.set(&Self::key("pending"), None)?;
        self.bus.publish(Topic::Sleep);
        Ok(log)
    }

    pub fn remind_enabled(&self) -> bool {
        self.settings.get(&Self::key("remind")).as_deref() != Some("0")
    }

    pub fn set_remind(&self, on: bool) -> AppResult {
        self.settings.set(&Self::key("remind"), Some(if on { "1" } else { "0" }))?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    /// Bedtime (minute of day) when it's time to wind down, once a day; else None.
    pub fn reminder(&self) -> Option<i32> {
        if !self.remind_enabled() || self.asleep_since().is_some() {
            return None;
        }
        let now = self.clock.now();
        let (hours, wake) = self.goal();
        let bedtime = bedtime_for(wake, hours);
        if self.reminded_on.get() == Some(now.date()) || !should_wind_down(minute_of(now), bedtime, WIND_DOWN_LEAD) {
            return None;
        }
        self.reminded_on.set(Some(now.date()));
        Some(bedtime)
    }

    pub fn night(&self, day: NaiveDate) -> AppResult<Option<SleepLog>> {
        self.logs.get(day)
    }

    pub fn save(&self, log: &SleepLog) -> AppResult {
        let log = SleepLog { note: log.note.trim().into(), ..log.clone() };
        log.validate()?;
        self.logs.put(&log)?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    /// Rate a night already logged, without touching its times.
    pub fn rate(&self, day: NaiveDate, quality: u8) -> AppResult {
        let mut log = self.logs.get(day)?.ok_or(ValidationError("Log the night first."))?;
        log.quality = Some(quality);
        log.validate()?;
        self.logs.put(&log)?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    pub fn delete(&self, day: NaiveDate) -> AppResult {
        self.logs.delete(day)?;
        self.bus.publish(Topic::Sleep);
        Ok(())
    }

    /// Average focus minutes on days after a night that reached the goal, and after one that didn't (last 30 days).
    fn focus_after(&self, today: NaiveDate, hours: f64) -> AppResult<(Option<i64>, Option<i64>)> {
        let Some(focus) = &self.focus else { return Ok((None, None)) };
        let first = today - Duration::days(29);
        let logs = self.logs.between(first, today)?;
        let mut per_day: HashMap<NaiveDate, i64> = HashMap::new();
        for s in focus.between(first, today)? {
            *per_day.entry(s.started.date()).or_default() += s.minutes;
        }
        if per_day.is_empty() {
            return Ok((None, None));
        }
        let goal = (hours * 60.0).round() * ENOUGH_RATIO;
        let side = |enough: bool| -> Vec<i64> {
            logs.iter().filter(|l| (f64::from(l.minutes()) >= goal) == enough).map(|l| *per_day.get(&l.day).unwrap_or(&0)).collect()
        };
        let (yes, no) = (side(true), side(false));
        if yes.len() < MIN_PER_GROUP || no.len() < MIN_PER_GROUP {
            return Ok((None, None));
        }
        let avg = |v: &[i64]| (v.iter().sum::<i64>() as f64 / v.len() as f64).round_ties_even() as i64;
        Ok((Some(avg(&yes)), Some(avg(&no))))
    }

    pub fn overview(&self, nights: i64) -> AppResult<SleepOverview> {
        let today = self.clock.today();
        let logs = self.logs.between(today - Duration::days(nights - 1), today)?;
        let (hours, wake) = self.goal();
        let stats = sleep_stats(&logs, hours);
        let recent: Vec<SleepLog> = logs.iter().filter(|l| l.day > today - Duration::days(7)).cloned().collect();
        let before: Vec<SleepLog> = logs.iter().filter(|l| l.day <= today - Duration::days(7)).cloned().collect();
        let last_week = (before.len() >= 3 && recent.len() >= 3).then(|| sleep_stats(&before, hours).average_minutes);
        let (after_enough, after_short) = self.focus_after(today, hours)?;
        let by_day: HashMap<NaiveDate, SleepLog> = logs.iter().map(|l| (l.day, l.clone())).collect();
        let days = (0..nights).rev().map(|i| { let d = today - Duration::days(i); (d, by_day.get(&d).cloned()) }).collect();
        let bedtime = bedtime_for(wake, hours);
        Ok(SleepOverview {
            goal_hours: hours, wake_goal: wake, bedtime_goal: bedtime, average_minutes: stats.average_minutes,
            average_bedtime: stats.average_bedtime, average_wake: stats.average_wake, average_quality: stats.average_quality,
            enough: stats.enough, shortfall_minutes: stats.shortfall_minutes, bedtime_spread: stats.bedtime_spread,
            regularity: if stats.nights >= 3 { regularity(stats.bedtime_spread) } else { "" }, streak: stats.streak,
            last_week_average: last_week, focus_after_enough: after_enough, focus_after_short: after_short,
            asleep_since: self.asleep_since().map(minute_of), days, last_night: by_day.get(&today).cloned(),
            bedtime_in: minutes_until(minute_of(self.clock.now()), bedtime), nights: logs,
        })
    }
}
