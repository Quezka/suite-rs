//! Medicines: what to take and when, what was taken, a nudge when one is due (port of `services/medicine.py`).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use chrono::{Duration, NaiveDate, Timelike};

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::{Clock, MedicineRepository};
use crate::domain::medicine::{due_now, is_late, plan_for, DoseLog, Medicine};

pub const WEEK: i64 = 7;

#[derive(Clone, Debug, PartialEq)]
pub struct DoseRecord {
    pub medicine_id: i64,
    pub name: String,
    pub dose: String,
    pub day: NaiveDate,
    /// Minute of the day; -1 for an extra one.
    pub planned: i32,
    /// Ticked, either way.
    pub done: bool,
    /// False with `done`: skipped on purpose.
    pub taken: bool,
    pub at: Option<i32>,
    pub late: bool,
}

pub struct MedicineService {
    pub repo: Rc<dyn MedicineRepository>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
    /// (medicine, day, planned) already nudged while Quire is open.
    pub reminded: RefCell<HashSet<(i64, NaiveDate, i32)>>,
}

impl MedicineService {
    fn minute(&self) -> i32 {
        let now = self.clock.now();
        (now.hour() * 60 + now.minute()) as i32
    }

    pub fn medicines(&self) -> AppResult<Vec<Medicine>> {
        self.repo.all()
    }

    pub fn medicine(&self, id: i64) -> AppResult<Medicine> {
        self.repo.get(id)?.ok_or_else(|| AppError::NotFound("That medicine no longer exists.".into()))
    }

    pub fn save(&self, id: Option<i64>, data: &Medicine) -> AppResult<i64> {
        let mut m = Medicine { name: data.name.trim().into(), dose: data.dose.trim().into(), id, ..data.clone() };
        m.times.sort();
        m.weekdays.sort();
        m.validate()?;
        let id = match id {
            None => self.repo.add(&m)?,
            Some(id) => {
                self.repo.update(&m)?;
                id
            }
        };
        self.bus.publish(Topic::Medicine);
        Ok(id)
    }

    pub fn delete(&self, id: i64) -> AppResult {
        self.repo.delete(id)?;
        self.bus.publish(Topic::Medicine);
        Ok(())
    }

    pub fn day(&self, day: Option<NaiveDate>) -> AppResult<Vec<DoseRecord>> {
        let today = self.clock.today();
        let day = day.unwrap_or(today);
        let minute = if day == today { self.minute() } else if day < today { 24 * 60 } else { -1 };
        Ok(plan_for(day, &self.repo.all()?, &self.repo.logs_between(day, day)?).iter().map(|d| DoseRecord {
            medicine_id: d.medicine.id.unwrap_or_default(), name: d.medicine.name.clone(), dose: d.medicine.dose.clone(),
            day: d.day, planned: d.planned, done: d.done(), taken: d.log.as_ref().is_some_and(|l| l.taken),
            at: d.log.as_ref().map(|l| l.at), late: is_late(d, minute),
        }).collect())
    }

    /// Today's doses not ticked yet, the next due first (extras left out).
    pub fn pending(&self) -> AppResult<Vec<DoseRecord>> {
        let mut out: Vec<DoseRecord> = self.day(None)?.into_iter().filter(|d| !d.done && d.planned >= 0).collect();
        out.sort_by_key(|d| d.planned);
        Ok(out)
    }

    /// Tick a dose as taken (or skipped). Doing it again changes it, never adds a second one.
    pub fn mark(&self, medicine_id: i64, day: NaiveDate, planned: i32, taken: bool) -> AppResult {
        let at = if day == self.clock.today() { self.minute() } else { planned.max(0) };
        for log in self.repo.logs_between(day, day)? {
            if log.medicine_id == medicine_id && log.planned == planned {
                self.repo.delete_log(log.id.unwrap_or_default())?;
                break;
            }
        }
        self.repo.add_log(&DoseLog { medicine_id, day, planned, taken, at, id: None })?;
        self.bus.publish(Topic::Medicine);
        Ok(())
    }

    pub fn unmark(&self, medicine_id: i64, day: NaiveDate, planned: i32) -> AppResult {
        for log in self.repo.logs_between(day, day)? {
            if log.medicine_id == medicine_id && log.planned == planned {
                self.repo.delete_log(log.id.unwrap_or_default())?;
            }
        }
        self.bus.publish(Topic::Medicine);
        Ok(())
    }

    /// Over the last seven days, today included: (planned doses, taken).
    pub fn week(&self) -> AppResult<(usize, usize)> {
        let today = self.clock.today();
        let first = today - Duration::days(WEEK - 1);
        let medicines = self.repo.all()?;
        let logs = self.repo.logs_between(first, today)?;
        let minute = self.minute();
        let (mut planned, mut taken) = (0, 0);
        for i in 0..WEEK {
            let day = first + Duration::days(i);
            for dose in plan_for(day, &medicines, &logs) {
                if day == today && !dose.done() && dose.planned > minute {
                    continue; // not time for it yet
                }
                if dose.planned >= 0 {
                    planned += 1;
                    taken += dose.log.as_ref().is_some_and(|l| l.taken) as usize;
                }
            }
        }
        Ok((planned, taken))
    }

    /// Doses to nudge about now, each only once. Call about every minute.
    pub fn due(&self) -> AppResult<Vec<DoseRecord>> {
        let today = self.clock.today();
        let minute = self.minute();
        let mut fresh = Vec::new();
        for d in plan_for(today, &self.repo.all()?, &self.repo.logs_between(today, today)?) {
            let key = (d.medicine.id.unwrap_or_default(), d.day, d.planned);
            if due_now(&d, minute) && self.reminded.borrow_mut().insert(key) {
                fresh.push(DoseRecord { medicine_id: key.0, name: d.medicine.name.clone(), dose: d.medicine.dose.clone(),
                    day: d.day, planned: d.planned, done: false, taken: false, at: None, late: false });
            }
        }
        Ok(fresh)
    }
}
