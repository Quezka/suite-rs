//! Sleep log rules (port of `quire/domain/sleep.py`). A night is logged on the day you woke up: when you fell asleep
//! and when you woke, as minutes after midnight. Falling asleep later in the day than waking means the evening before.

use chrono::{Duration, NaiveDate};

use super::errors::{Checked, ValidationError};

pub const MINUTES_PER_DAY: i32 = 24 * 60;
/// Shorter than this is a nap, not a night.
pub const MIN_SLEEP: i32 = 30;
pub const MAX_SLEEP: i32 = 18 * 60;
pub const DEFAULT_GOAL_HOURS: f64 = 8.0;
pub const DEFAULT_WAKE: i32 = 6 * 60;
/// A night counts as enough from 90% of the goal.
pub const ENOUGH_RATIO: f64 = 0.9;
/// Bedtimes within about half an hour: steady.
pub const REGULAR_SPREAD: i32 = 30;
pub const LOOSE_SPREAD: i32 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SleepLog {
    /// The day you woke up.
    pub day: NaiveDate,
    /// Minutes after midnight.
    pub asleep: i32,
    pub woke: i32,
    /// 1 (awful) to 5 (great); None when not rated.
    pub quality: Option<u8>,
    pub note: String,
}

impl SleepLog {
    pub fn validate(&self) -> Checked {
        if !((0..MINUTES_PER_DAY).contains(&self.asleep) && (0..MINUTES_PER_DAY).contains(&self.woke)) {
            return Err(ValidationError("Times must fall within a single day."));
        }
        if self.quality.is_some_and(|q| !(1..=5).contains(&q)) {
            return Err(ValidationError("Rate the night from 1 to 5."));
        }
        if !(MIN_SLEEP..=MAX_SLEEP).contains(&self.minutes()) {
            return Err(ValidationError("A night's sleep is between 30 minutes and 18 hours."));
        }
        Ok(())
    }

    pub fn minutes(&self) -> i32 {
        (self.woke - self.asleep).rem_euclid(MINUTES_PER_DAY)
    }

    /// The calendar day the night began.
    pub fn fell_asleep_on(&self) -> NaiveDate {
        if self.asleep > self.woke { self.day - Duration::days(1) } else { self.day }
    }
}

fn goal_minutes(hours: f64) -> i32 {
    (hours * 60.0).round_ties_even() as i32
}

/// When to be asleep to wake at `wake` after `hours` of sleep.
pub fn bedtime_for(wake: i32, hours: f64) -> i32 {
    (wake - goal_minutes(hours)).rem_euclid(MINUTES_PER_DAY)
}

/// Minutes from `now` to the next time the clock shows `target`.
pub fn minutes_until(now: i32, target: i32) -> i32 {
    (target - now).rem_euclid(MINUTES_PER_DAY)
}

/// Bedtimes either side of midnight average sensibly when measured from noon.
fn evening_minutes(minute: i32) -> i32 {
    (minute - 12 * 60).rem_euclid(MINUTES_PER_DAY)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SleepStats {
    pub nights: usize,
    pub average_minutes: i32,
    pub average_bedtime: Option<i32>,
    pub average_wake: Option<i32>,
    pub average_quality: Option<f64>,
    /// Nights that reached the goal.
    pub enough: usize,
    /// Total missing against the goal over the period.
    pub shortfall_minutes: i32,
    /// How far bedtimes wander, in minutes (standard deviation).
    pub bedtime_spread: i32,
    /// Nights in a row, up to the latest, that reached the goal.
    pub streak: usize,
}

/// "steady", "mixed" or "irregular", from the spread of bedtimes.
pub fn regularity(spread: i32) -> &'static str {
    if spread <= REGULAR_SPREAD {
        "steady"
    } else if spread <= LOOSE_SPREAD {
        "mixed"
    } else {
        "irregular"
    }
}

pub fn streak_of(logs: &[SleepLog], goal_hours: f64) -> usize {
    let goal = f64::from(goal_minutes(goal_hours));
    let mut sorted: Vec<&SleepLog> = logs.iter().collect();
    sorted.sort_by(|a, b| b.day.cmp(&a.day));
    let (mut run, mut previous) = (0, None::<NaiveDate>);
    for log in sorted {
        if f64::from(log.minutes()) < goal * ENOUGH_RATIO || previous.is_some_and(|p| (p - log.day).num_days() != 1) {
            break;
        }
        run += 1;
        previous = Some(log.day);
    }
    run
}

/// Within `lead` minutes before bedtime (measured around midnight).
pub fn should_wind_down(minute: i32, bedtime: i32, lead: i32) -> bool {
    let ahead = (bedtime - minute).rem_euclid(MINUTES_PER_DAY);
    0 < ahead && ahead <= lead
}

pub fn sleep_stats(logs: &[SleepLog], goal_hours: f64) -> SleepStats {
    if logs.is_empty() {
        return SleepStats {
            nights: 0,
            average_minutes: 0,
            average_bedtime: None,
            average_wake: None,
            average_quality: None,
            enough: 0,
            shortfall_minutes: 0,
            bedtime_spread: 0,
            streak: 0,
        };
    }
    let goal = goal_minutes(goal_hours);
    let n = logs.len() as i32;
    let evenings: Vec<i32> = logs.iter().map(|l| evening_minutes(l.asleep)).collect();
    let bed = evenings.iter().sum::<i32>() / n;
    let wake = logs.iter().map(|l| l.woke).sum::<i32>() / n;
    let rated: Vec<f64> = logs.iter().filter_map(|l| l.quality.filter(|q| *q > 0)).map(f64::from).collect();
    let mean = f64::from(evenings.iter().sum::<i32>()) / f64::from(n);
    let variance = evenings.iter().map(|e| (f64::from(*e) - mean).powi(2)).sum::<f64>() / f64::from(n);
    SleepStats {
        nights: logs.len(),
        average_minutes: logs.iter().map(SleepLog::minutes).sum::<i32>() / n,
        average_bedtime: Some((bed + 12 * 60).rem_euclid(MINUTES_PER_DAY)),
        average_wake: Some(wake),
        average_quality: (!rated.is_empty()).then(|| rated.iter().sum::<f64>() / rated.len() as f64),
        enough: logs.iter().filter(|l| f64::from(l.minutes()) >= f64::from(goal) * ENOUGH_RATIO).count(),
        shortfall_minutes: logs.iter().map(|l| (goal - l.minutes()).max(0)).sum(),
        bedtime_spread: variance.sqrt().round_ties_even() as i32,
        streak: streak_of(logs, goal_hours),
    }
}
