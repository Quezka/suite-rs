//! Water tracking rules (port of `quire/domain/water.py`).
//!
//! The daily goal comes from body surface area (Mosteller: sqrt(height cm × weight kg / 3600) square metres), 1200
//! ml per square metre, rounded to 50 ml: about 2.2 L for 175 cm and 70 kg. A rough guide, not medical advice.

use chrono::NaiveDate;

use super::errors::{Checked, ValidationError};

pub const ML_PER_SQUARE_METRE: f64 = 1200.0;
pub const MIN_GOAL: i64 = 1200;
pub const MAX_GOAL: i64 = 4500;
pub const MIN_MANUAL_GOAL: i64 = 500;
pub const MAX_MANUAL_GOAL: i64 = 6000;
/// One entry; more is a typo.
pub const MAX_SIP: i64 = 2000;
pub const DEFAULT_GLASS: i64 = 250;
/// Reminders run between these times of day.
pub const DEFAULT_START: i32 = 8 * 60;
pub const DEFAULT_END: i32 = 22 * 60;
/// Remind when this far behind the pace.
pub const BEHIND_ML: i64 = 250;
/// Minutes between reminders at the least.
pub const REMIND_EVERY: i32 = 90;

pub fn water_goal_ml(height_cm: f64, weight_kg: f64) -> Checked<i64> {
    if !((80.0..=250.0).contains(&height_cm) && (20.0..=300.0).contains(&weight_kg)) {
        return Err(ValidationError("Enter a height in cm and a weight in kg."));
    }
    let area = (height_cm * weight_kg / 3600.0).sqrt();
    let goal = (area * ML_PER_SQUARE_METRE / 50.0).round_ties_even() as i64 * 50;
    Ok(goal.clamp(MIN_GOAL, MAX_GOAL))
}

pub fn check_manual_goal(goal: i64) -> Checked<i64> {
    if !(MIN_MANUAL_GOAL..=MAX_MANUAL_GOAL).contains(&goal) {
        return Err(ValidationError("Enter a daily goal between 500 and 6000 ml."));
    }
    Ok(goal)
}

/// How many days in a row reached the goal. `days` holds each day's ml, newest first, today first: today doesn't
/// break the run while it can still be reached.
pub fn water_streak(days: &[i64], goal: i64) -> usize {
    let mut run = 0;
    for (i, ml) in days.iter().enumerate() {
        if *ml >= goal {
            run += 1;
        } else if i > 0 {
            break;
        }
    }
    run
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaterLog {
    pub day: NaiveDate,
    /// Minutes after midnight.
    pub at: i32,
    pub ml: i64,
    pub id: Option<i64>,
}

impl WaterLog {
    pub fn validate(&self) -> Checked {
        if !(0..24 * 60).contains(&self.at) {
            return Err(ValidationError("Times must fall within a single day."));
        }
        if !(1..=MAX_SIP).contains(&self.ml) {
            return Err(ValidationError("Enter an amount between 1 and 2000 ml."));
        }
        Ok(())
    }
}

/// How much should have been drunk by `minute`, spread evenly across the waking day.
pub fn expected_ml(goal: i64, minute: i32, start: i32, end: i32) -> i64 {
    if minute <= start {
        return 0;
    }
    if minute >= end {
        return goal;
    }
    (goal as f64 * f64::from(minute - start) / f64::from(end - start)).round_ties_even() as i64
}

/// Remind while you're awake, behind the pace, below the goal, and not just reminded.
pub fn reminder_due(goal: i64, drunk: i64, minute: i32, last_reminder: Option<i32>, start: i32, end: i32) -> bool {
    if !(start..end).contains(&minute) || drunk >= goal {
        return false;
    }
    if last_reminder.is_some_and(|last| (0..REMIND_EVERY).contains(&(minute - last))) {
        return false;
    }
    expected_ml(goal, minute, start, end) - drunk >= BEHIND_ML
}
