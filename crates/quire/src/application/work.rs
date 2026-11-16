//! Jobs and work shifts, hours and pay (port of `services/work.py`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate, Timelike};

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::planner::{AgendaItem, ItemKind};
use super::ports::{Clock, JobRepository, ShiftRepository};
use crate::domain::model::{reschedule, work_shifts, Job, Shift, ShiftPattern, TimeRange};
use crate::domain::payroll::{monthly_gross, monthly_share, round2, take_home, year_payslips, Payslip};
use crate::domain::ValidationError;

pub const MAX_REPEAT_WEEKS: i64 = 52;

#[derive(Clone, Debug)]
pub struct PatternInput {
    pub weekday: u8,
    pub start: i32,
    pub end: i32,
    pub break_minutes: i32,
}

#[derive(Clone, Debug)]
pub struct ShiftInput {
    pub job_id: i64,
    pub day: NaiveDate,
    pub start: i32,
    pub end: i32,
    pub break_minutes: i32,
    pub notes: String,
}

#[derive(Clone, Debug)]
pub struct ShiftItem {
    pub shift: Shift,
    pub job: Option<Job>,
    pub pay: Option<f64>,
    pub net: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct JobTotal {
    pub job: Option<Job>,
    pub minutes: i32,
    pub pay: Option<f64>,
    pub net: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct WorkSummary {
    pub first: NaiveDate,
    pub last: NaiveDate,
    pub minutes: i32,
    pub pay: Option<f64>,
    pub shift_count: usize,
    pub per_job: Vec<JobTotal>,
    pub net: Option<f64>,
}

type SlipCache = RefCell<HashMap<(i64, i32), Vec<Payslip>>>;

pub struct WorkService {
    pub jobs: Rc<dyn JobRepository>,
    pub shifts: Rc<dyn ShiftRepository>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
}

fn from_input(d: &ShiftInput, id: Option<i64>) -> Shift {
    Shift { break_minutes: d.break_minutes, notes: d.notes.clone(), id, ..Shift::between(d.job_id, d.day, d.start, d.end) }
}

/// Gross pay of one shift: only hourly jobs pay by the shift.
fn shift_pay(job: Option<&Job>, s: &Shift) -> Option<f64> {
    job.filter(|j| j.pay_mode == "hourly").and_then(|j| s.pay(j.hourly_rate))
}

impl WorkService {
    pub fn jobs(&self) -> AppResult<Vec<Job>> {
        self.jobs.list()
    }

    pub fn job(&self, id: i64) -> AppResult<Job> {
        self.jobs.list()?.into_iter().find(|j| j.id == Some(id)).ok_or_else(|| AppError::NotFound(format!("Job {id} does not exist.")))
    }

    fn this_monday(&self) -> NaiveDate {
        let today = self.clock.today();
        today - Duration::days(today.weekday().num_days_from_monday() as i64)
    }

    /// Create or change a job (`data`'s fields); `weekly` replaces its weekly schedule from this week on.
    pub fn save_job(&self, id: Option<i64>, data: &Job, weekly: Option<&[PatternInput]>) -> AppResult<i64> {
        let mut job = match id { None => Job::named(&data.name), Some(id) => self.job(id)? };
        let schedule = std::mem::take(&mut job.schedule);
        job = Job { name: data.name.trim().into(), id: job.id, schedule, ..data.clone() };
        if let Some(weekly) = weekly {
            let wanted: Vec<ShiftPattern> = weekly.iter().map(|p| ShiftPattern::between(p.weekday, p.start, p.end, p.break_minutes)).collect();
            job.schedule = reschedule(&job.schedule, &wanted, self.this_monday());
        }
        job.validate()?;
        let id = match job.id {
            None => self.jobs.add(&job)?,
            Some(id) => {
                self.jobs.update(&job)?;
                id
            }
        };
        self.bus.publish(Topic::Work);
        Ok(id)
    }

    /// Delete a job together with all of its shifts.
    pub fn delete_job(&self, id: i64) -> AppResult {
        self.jobs.delete(id)?;
        self.bus.publish(Topic::Work);
        Ok(())
    }

    pub fn shift(&self, id: i64) -> AppResult<Shift> {
        self.shifts.get(id)?.ok_or_else(|| AppError::NotFound(format!("Shift {id} does not exist.")))
    }

    /// Save a shift; a new one can be copied to the following weeks. `replaces` names a weekly-schedule occurrence
    /// (job, day, start) this one stands in for.
    pub fn save_shift(&self, id: Option<i64>, data: &ShiftInput, repeat_weeks: i64, replaces: Option<(i64, NaiveDate, i32)>) -> AppResult<Vec<i64>> {
        if let Some(id) = id {
            self.shift(id)?; // must exist
        }
        let s = from_input(data, id);
        s.validate()?;
        if self.job(s.job_id).is_err() {
            return Err(ValidationError("Pick the job this shift is for.").into());
        }
        if !(0..=MAX_REPEAT_WEEKS).contains(&repeat_weeks) {
            return Err(ValidationError("Repeat for at most 52 weeks.").into());
        }
        let ids = match s.id {
            Some(id) => {
                if repeat_weeks > 0 {
                    return Err(ValidationError("Only new shifts can be repeated.").into());
                }
                self.shifts.update(&s)?;
                vec![id]
            }
            None => {
                let mut ids = vec![self.shifts.add(&s)?];
                for week in 1..=repeat_weeks {
                    ids.push(self.shifts.add(&Shift { day: s.day + Duration::weeks(week), id: None, ..s.clone() })?);
                }
                ids
            }
        };
        if let Some((job, day, start)) = replaces {
            self.shifts.skip(job, day, start)?;
        }
        self.bus.publish(Topic::Work);
        Ok(ids)
    }

    /// Make a shift repeat on the given weekdays from `since`; returns how many patterns were added.
    pub fn add_weekly(&self, job_id: i64, weekdays: &[u8], start: i32, end: i32, break_minutes: i32,
                      since: Option<NaiveDate>, until: Option<NaiveDate>) -> AppResult<usize> {
        let mut job = self.job(job_id)?;
        let mut days = weekdays.to_vec();
        days.sort();
        days.dedup();
        if days.is_empty() {
            return Err(ValidationError("Pick at least one day for the shift to repeat on.").into());
        }
        let today = self.clock.today();
        let since = since.unwrap_or(today);
        let mut taken: std::collections::HashSet<_> = job.active_schedule(today).iter().map(|p| p.key()).collect();
        let mut added = 0;
        for weekday in days {
            let p = ShiftPattern { since: Some(since), until, ..ShiftPattern::between(weekday, start, end, break_minutes) };
            p.validate()?;
            if taken.insert(p.key()) {
                job.schedule.push(p);
                added += 1;
            }
        }
        self.jobs.update(&job)?;
        self.bus.publish(Topic::Work);
        Ok(added)
    }

    pub fn skip_occurrence(&self, job_id: i64, day: NaiveDate, start: i32) -> AppResult {
        self.shifts.skip(job_id, day, start)?;
        self.bus.publish(Topic::Work);
        Ok(())
    }

    pub fn delete_shift(&self, id: i64) -> AppResult {
        self.shifts.delete(id)?;
        self.bus.publish(Topic::Work);
        Ok(())
    }

    // ---- pay ----

    /// The year's twelve payslips of a job (worked out once per request).
    fn slips(&self, job: &Job, year: i32, cache: &SlipCache) -> AppResult<Vec<Payslip>> {
        let key = (job.id.unwrap_or_default(), year);
        if let Some(s) = cache.borrow().get(&key) {
            return Ok(s.clone());
        }
        let terms = job.terms();
        let mut grosses = [(0.0, 0.0); 12];
        let mut worked = Vec::new();
        if terms.pay_mode == "monthly" {
            for m in 1..=12u32 {
                grosses[m as usize - 1] = monthly_gross(&terms, year, m);
            }
        } else {
            let (first, last) = (NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), NaiveDate::from_ymd_opt(year, 12, 31).unwrap());
            let shifts: Vec<Shift> = work_shifts(std::slice::from_ref(job), &self.shifts.starting_between(first, last)?,
                                                 &self.shifts.skipped(first, last)?, first, last)
                .into_iter().filter(|s| Some(s.job_id) == job.id && s.day.year() == year).collect();
            let mut totals = [0.0f64; 12];
            for s in &shifts {
                totals[s.day.month0() as usize] += s.pay(job.hourly_rate).unwrap_or(0.0);
            }
            for (i, t) in totals.iter().enumerate() {
                grosses[i] = (round2(*t), 0.0);
            }
            worked = shifts.iter().map(|s| s.day).collect();
        }
        let slips = year_payslips(&terms, year, &grosses, &worked);
        cache.borrow_mut().insert(key, slips.clone());
        Ok(slips)
    }

    fn shift_net(&self, job: Option<&Job>, s: &Shift, gross: Option<f64>, cache: &SlipCache) -> AppResult<Option<f64>> {
        match (gross, job) {
            (Some(g), Some(j)) => Ok(Some(take_home(&j.terms(), &self.slips(j, s.day.year(), cache)?[s.day.month0() as usize], g))),
            _ => Ok(gross),
        }
    }

    /// The months of `year` in which the job pays something.
    pub fn payslips(&self, job_id: i64, year: i32) -> AppResult<Vec<Payslip>> {
        let job = self.job(job_id)?;
        Ok(self.slips(&job, year, &RefCell::default())?.into_iter().filter(|p| p.gross > 0.0).collect())
    }

    /// One-off shifts and weekly-schedule occurrences starting on first..=last.
    pub fn shifts_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<ShiftItem>> {
        let jobs = self.jobs.list()?;
        let shifts = work_shifts(&jobs, &self.shifts.starting_between(first, last)?, &self.shifts.skipped(first, last)?, first, last);
        let cache = SlipCache::default();
        shifts.into_iter().map(|s| {
            let job = jobs.iter().find(|j| j.id == Some(s.job_id));
            let pay = shift_pay(job, &s);
            let net = self.shift_net(job, &s, pay, &cache)?;
            Ok(ShiftItem { job: job.cloned(), pay, net, shift: s })
        }).collect()
    }

    /// Shifts that haven't ended yet, soonest first.
    pub fn upcoming(&self, days: i64) -> AppResult<Vec<ShiftItem>> {
        let now = self.clock.now();
        let today = now.date();
        let minute = (now.hour() * 60 + now.minute()) as i64;
        Ok(self.shifts_between(today - Duration::days(1), today + Duration::days(days))?.into_iter()
            .filter(|i| (i.shift.day - today).num_days() * 1440 + i.shift.end() as i64 > minute).collect())
    }

    fn pay_between(&self, job: &Job, items: &[ShiftItem], first: NaiveDate, last: NaiveDate, cache: &SlipCache) -> AppResult<(Option<f64>, Option<f64>)> {
        let terms = job.terms();
        if terms.pay_mode == "monthly" {
            if terms.monthly_pay.is_none() {
                return Ok((None, None));
            }
            let (mut gross, mut net) = (0.0, 0.0);
            let (mut year, mut month) = (first.year(), first.month());
            while (year, month) <= (last.year(), last.month()) {
                let (g, n) = monthly_share(&terms, &self.slips(job, year, cache)?[month as usize - 1], first, last);
                gross += g;
                net += n;
                if month == 12 { year += 1; month = 1 } else { month += 1 }
            }
            return Ok((Some(round2(gross)), Some(round2(net))));
        }
        if items.iter().any(|i| i.pay.is_none()) {
            return Ok((None, None));
        }
        let mut by_month: Vec<((i32, u32), f64)> = Vec::new();
        for i in items {
            let key = (i.shift.day.year(), i.shift.day.month());
            match by_month.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => *v += i.pay.unwrap_or(0.0),
                None => by_month.push((key, i.pay.unwrap_or(0.0))),
            }
        }
        let mut net = 0.0;
        for ((y, m), g) in &by_month {
            net += take_home(&terms, &self.slips(job, *y, cache)?[*m as usize - 1], *g);
        }
        Ok((Some(round2(by_month.iter().map(|(_, g)| g).sum())), Some(round2(net))))
    }

    /// Paid hours and estimated pay for shifts starting in first..=last.
    pub fn summary(&self, first: NaiveDate, last: NaiveDate) -> AppResult<WorkSummary> {
        let items = self.shifts_between(first, last)?;
        let jobs = self.jobs.list()?;
        let mut per_job: Vec<(i64, Vec<ShiftItem>)> = Vec::new();
        for item in &items {
            match per_job.iter_mut().find(|(id, _)| *id == item.shift.job_id) {
                Some((_, v)) => v.push(item.clone()),
                None => per_job.push((item.shift.job_id, vec![item.clone()])),
            }
        }
        for j in &jobs {
            // a monthly job is paid even in a week without shifts
            if j.pay_mode == "monthly" && j.monthly_pay.is_some() && !per_job.iter().any(|(id, _)| Some(*id) == j.id) {
                per_job.push((j.id.unwrap_or_default(), Vec::new()));
            }
        }
        let cache = SlipCache::default();
        let mut totals = Vec::new();
        for (id, job_items) in &per_job {
            let job = jobs.iter().find(|j| j.id == Some(*id));
            let (pay, net) = match job { Some(j) => self.pay_between(j, job_items, first, last, &cache)?, None => (None, None) };
            totals.push(JobTotal { job: job.cloned(), minutes: job_items.iter().map(|i| i.shift.paid_minutes()).sum(), pay, net });
        }
        totals.retain(|t| t.minutes != 0 || t.pay.is_some_and(|p| p != 0.0));
        totals.sort_by_key(|t| t.job.as_ref().map(|j| j.name.to_lowercase()).unwrap_or_default());
        let gross: Vec<f64> = totals.iter().filter_map(|t| t.pay).collect();
        let net: Vec<f64> = totals.iter().filter_map(|t| t.net).collect();
        Ok(WorkSummary {
            first, last, minutes: totals.iter().map(|t| t.minutes).sum(),
            pay: (!gross.is_empty()).then(|| round2(gross.iter().sum())), shift_count: items.len(),
            net: (!net.is_empty()).then(|| round2(net.iter().sum())), per_job: totals,
        })
    }

    pub fn week_summary(&self) -> AppResult<WorkSummary> {
        let monday = self.this_monday();
        self.summary(monday, monday + Duration::days(6))
    }

    pub fn month_summary(&self) -> AppResult<WorkSummary> {
        let today = self.clock.today();
        let first = today.with_day(1).unwrap();
        let next = (first + Duration::days(32)).with_day(1).unwrap();
        self.summary(first, next - Duration::days(1))
    }

    /// Items that overlap a shift being edited; the shift itself and the weekly occurrence it `replaces` don't count.
    pub fn clashes(&self, id: Option<i64>, data: &ShiftInput, agenda: &[AgendaItem], replaces: Option<(i64, NaiveDate, i32)>) -> Vec<AgendaItem> {
        let s = from_input(data, id);
        let is_self = |i: &AgendaItem| match i.kind {
            ItemKind::Shift => s.id.is_some() && i.ref_id == s.id,
            ItemKind::WeeklyShift => replaces.is_some_and(|(j, d, st)| i.ref_id == Some(j) && i.origin == Some((d, st))),
            _ => false,
        };
        agenda.iter().filter(|i| !is_self(i) && s.overlaps(i.day, TimeRange { start: i.time.start, end: i.time.end })).cloned().collect()
    }
}
