//! Medicine tracking rules (port of `quire/domain/medicine.py`). It remembers what you told it to: it gives no medical
//! advice.

use chrono::NaiveDate;

use super::errors::{Checked, ValidationError};
use super::model::weekday;

pub const MAX_DOSES_A_DAY: usize = 12;
/// Minutes after the time before a dose counts as late.
pub const LATE_AFTER: i32 = 60;
/// Minutes after the time it is still worth reminding.
pub const REMIND_FOR: i32 = 180;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Medicine {
    pub name: String,
    /// Free text: "1 tablet", "5 ml".
    pub dose: String,
    /// Minutes after midnight.
    pub times: Vec<i32>,
    /// 0 = Monday; none chosen means every day.
    pub weekdays: Vec<u8>,
    pub active: bool,
    pub notes: String,
    pub id: Option<i64>,
}

impl Medicine {
    pub fn named(name: &str) -> Self {
        Self { name: name.into(), dose: String::new(), times: Vec::new(), weekdays: Vec::new(), active: true, notes: String::new(), id: None }
    }

    pub fn validate(&self) -> Checked {
        if self.name.trim().is_empty() {
            return Err(ValidationError("Give the medicine a name."));
        }
        if self.times.len() > MAX_DOSES_A_DAY {
            return Err(ValidationError("That is more than 12 times a day."));
        }
        let mut unique = self.times.clone();
        unique.sort_unstable();
        unique.dedup();
        if self.times.iter().any(|t| !(0..24 * 60).contains(t)) || unique.len() != self.times.len() {
            return Err(ValidationError("Each time must be a different time of day."));
        }
        if self.weekdays.iter().any(|d| *d > 6) {
            return Err(ValidationError("Choose days of the week."));
        }
        Ok(())
    }

    pub fn due_on(&self, day: NaiveDate) -> bool {
        self.active && (self.weekdays.is_empty() || self.weekdays.contains(&weekday(day)))
    }
}

/// A dose you ticked: taken, or deliberately skipped. A planned dose with no log is still open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoseLog {
    pub medicine_id: i64,
    pub day: NaiveDate,
    /// The time of day it was planned for; -1 for an extra one, outside the plan.
    pub planned: i32,
    pub taken: bool,
    /// Minutes after midnight when you ticked it.
    pub at: i32,
    pub id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedDose {
    pub medicine: Medicine,
    pub day: NaiveDate,
    pub planned: i32,
    pub log: Option<DoseLog>,
}

impl PlannedDose {
    pub fn done(&self) -> bool {
        self.log.is_some()
    }
}

/// Every dose of a day in time order, each with its log if it has one. A dose already logged shows even if the
/// medicine has since been changed or stopped: what you took stays true.
pub fn plan_for(day: NaiveDate, medicines: &[Medicine], logs: &[DoseLog]) -> Vec<PlannedDose> {
    // Logs of the day by (medicine, planned time), the later one winning, in first-seen order (like a Python dict).
    let mut by_key: Vec<((i64, i32), DoseLog)> = Vec::new();
    for log in logs.iter().filter(|l| l.day == day) {
        let key = (log.medicine_id, log.planned);
        match by_key.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = log.clone(),
            None => by_key.push((key, log.clone())),
        }
    }
    let mut plan = Vec::new();
    for m in medicines.iter().filter(|m| m.due_on(day)) {
        for t in &m.times {
            let log = m.id.and_then(|id| by_key.iter().position(|(k, _)| *k == (id, *t))).map(|i| by_key.remove(i).1);
            plan.push(PlannedDose { medicine: m.clone(), day, planned: *t, log });
        }
    }
    for ((medicine_id, planned), log) in by_key {
        if let Some(m) = medicines.iter().find(|m| m.id == Some(medicine_id)) {
            plan.push(PlannedDose { medicine: m.clone(), day, planned, log: Some(log) });
        }
    }
    let when = |d: &PlannedDose| if d.planned >= 0 { d.planned } else { d.log.as_ref().map_or(0, |l| l.at) };
    plan.sort_by(|a, b| when(a).cmp(&when(b)).then_with(|| a.medicine.name.to_lowercase().cmp(&b.medicine.name.to_lowercase())));
    plan
}

pub fn is_late(dose: &PlannedDose, minute: i32) -> bool {
    !dose.done() && dose.planned >= 0 && minute > dose.planned + LATE_AFTER
}

/// Open, its time has come, and not so long ago that a reminder would be pointless.
pub fn due_now(dose: &PlannedDose, minute: i32) -> bool {
    !dose.done() && dose.planned >= 0 && dose.planned <= minute && minute <= dose.planned + REMIND_FOR
}
