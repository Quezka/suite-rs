//! The day planner and the week view (port of `services/planner.py`).

use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate};

use super::bus::{ChangeBus, Topic};
use super::errors::AppResult;
use super::ports::{Clock, CourseRepository, EventRepository, JobRepository, JournalRepository, RoutineRepository,
                   ShiftRepository, TaskRepository};
use super::tasks::TaskItem;
use crate::domain::model::{milestone_time, weekday, work_shifts, Course, Event, Routine, TimeRange};

pub const UPCOMING_TASK_DAYS: i64 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Class,
    Event,
    /// A repeating milestone; `ref_id` is the routine id.
    Routine,
    /// A one-off shift; `ref_id` is the shift id.
    Shift,
    /// From a job's weekly schedule; `ref_id` is the job id.
    WeeklyShift,
}

impl ItemKind {
    /// Python's `ItemKind.value`: kept in calendar uids, so they stay the same across apps.
    pub fn name(self) -> &'static str {
        match self {
            ItemKind::Class => "class",
            ItemKind::Event => "event",
            ItemKind::Routine => "routine",
            ItemKind::Shift => "shift",
            ItemKind::WeeklyShift => "weekly_shift",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgendaItem {
    pub kind: ItemKind,
    pub ref_id: Option<i64>,
    pub day: NaiveDate,
    pub time: TimeRange,
    pub title: String,
    pub color: String,
    pub room: String,
    pub teacher: String,
    pub details: String,
    /// For items not stored on their own (weekly shifts) or spanning midnight: the day and minute the whole starts.
    pub origin: Option<(NaiveDate, i32)>,
}

impl AgendaItem {
    pub fn is_all_day(&self) -> bool {
        self.kind == ItemKind::Event && self.time.end - self.time.start == crate::domain::model::MINUTES_PER_DAY
    }

    pub fn is_milestone(&self) -> bool {
        matches!(self.kind, ItemKind::Event | ItemKind::Routine) && self.time.end - self.time.start == crate::domain::model::MILESTONE_MINUTES
    }

    fn new(kind: ItemKind, ref_id: Option<i64>, day: NaiveDate, time: TimeRange, title: &str, color: &str) -> Self {
        Self { kind, ref_id, day, time, title: title.into(), color: color.into(), room: String::new(),
               teacher: String::new(), details: String::new(), origin: None }
    }
}

#[derive(Clone, Debug)]
pub struct DayAgenda {
    pub day: NaiveDate,
    pub is_today: bool,
    pub items: Vec<AgendaItem>,
    pub tasks: Vec<TaskItem>,
    pub journal: String,
    pub has_courses: bool,
}

impl DayAgenda {
    pub fn class_count(&self) -> usize {
        self.items.iter().filter(|i| i.kind == ItemKind::Class).count()
    }
    pub fn event_count(&self) -> usize {
        self.items.iter().filter(|i| i.kind == ItemKind::Event).count()
    }
}

pub fn monday_of(day: NaiveDate) -> NaiveDate {
    day - Duration::days(day.weekday().num_days_from_monday() as i64)
}

fn class_items(courses: &[Course], day: NaiveDate) -> Vec<AgendaItem> {
    courses.iter().flat_map(|c| c.slots_on(weekday(day)).into_iter().map(move |slot| AgendaItem {
        room: c.room_for(slot).to_string(), teacher: c.teacher.clone(),
        ..AgendaItem::new(ItemKind::Class, c.id, day, slot.time, &c.name, &c.color)
    })).collect()
}

fn event_item(e: &Event) -> AgendaItem {
    AgendaItem { details: e.details.clone(), ..AgendaItem::new(ItemKind::Event, e.id, e.day, e.time, &e.title, &e.color) }
}

fn routine_items(routines: &[Routine], day: NaiveDate) -> Vec<AgendaItem> {
    routines.iter().filter(|r| r.occurs_on(day))
        .map(|r| AgendaItem::new(ItemKind::Routine, r.id, day, milestone_time(r.minute), &r.title, &r.color)).collect()
}

pub struct PlannerService {
    pub courses: Rc<dyn CourseRepository>,
    pub events: Rc<dyn EventRepository>,
    pub tasks: Rc<dyn TaskRepository>,
    pub journal: Rc<dyn JournalRepository>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
    pub jobs: Rc<dyn JobRepository>,
    pub shifts: Rc<dyn ShiftRepository>,
    pub routines: Rc<dyn RoutineRepository>,
}

impl PlannerService {
    pub fn today(&self) -> NaiveDate {
        self.clock.today()
    }

    /// Shift segments on days first..=last (a shift may start the evening before).
    fn shift_items(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<AgendaItem>> {
        let jobs = self.jobs.list()?;
        let start = first - Duration::days(1);
        let shifts = work_shifts(&jobs, &self.shifts.starting_between(start, last)?, &self.shifts.skipped(start, last)?, start, last);
        let mut items = Vec::new();
        for s in shifts {
            let job = jobs.iter().find(|j| j.id == Some(s.job_id));
            let (kind, ref_id) = if s.recurring { (ItemKind::WeeklyShift, Some(s.job_id)) } else { (ItemKind::Shift, s.id) };
            for (day, time) in s.segments() {
                if first <= day && day <= last {
                    items.push(AgendaItem { details: s.notes.clone(), origin: Some((s.day, s.start)),
                        ..AgendaItem::new(kind, ref_id, day, time, job.map_or("Shift", |j| &j.name),
                                          job.map_or("#0090ff", |j| &j.color)) });
                }
            }
        }
        Ok(items)
    }

    /// Every class, routine, event and shift segment on days first..=last.
    pub fn items_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<AgendaItem>> {
        let courses = self.courses.list()?;
        let routines = self.routines.list()?;
        let days: Vec<NaiveDate> = (0..=(last - first).num_days()).map(|n| first + Duration::days(n)).collect();
        let mut items: Vec<AgendaItem> = days.iter().flat_map(|d| class_items(&courses, *d)).collect();
        items.extend(days.iter().flat_map(|d| routine_items(&routines, *d)));
        items.extend(self.events.between(first, last)?.iter().map(event_item));
        items.extend(self.shift_items(first, last)?);
        Ok(items)
    }

    pub fn day_agenda(&self, day: NaiveDate) -> AppResult<DayAgenda> {
        let today = self.clock.today();
        let courses = self.courses.list()?;
        let mut items = class_items(&courses, day);
        items.extend(routine_items(&self.routines.list()?, day));
        items.extend(self.events.between(day, day)?.iter().map(event_item));
        items.extend(self.shift_items(day, day)?);
        items.sort_by_key(|i| (i.time.start, i.time.end));

        let all = self.tasks.list(true, None)?;
        let due_on = |d: NaiveDate| all.iter().filter(move |t| t.due == Some(d)).cloned();
        let mut tasks: Vec<_> = due_on(day).collect();
        if day == today {
            // today: unfinished from earlier, today's, and the next UPCOMING_TASK_DAYS days
            let before = all.iter().filter(|t| !t.done && t.due.is_some_and(|d| d < day)).cloned();
            let mut ahead: Vec<_> = all.iter().filter(|t| !t.done && t.due.is_some_and(|d| {
                let n = (d - day).num_days();
                n > 0 && n <= UPCOMING_TASK_DAYS
            })).cloned().collect();
            ahead.sort_by(|a, b| (a.due, a.title.to_lowercase()).cmp(&(b.due, b.title.to_lowercase())));
            tasks = before.chain(tasks).chain(ahead).collect();
        }
        let task_items = tasks.into_iter().map(|t| TaskItem {
            course: courses.iter().find(|c| t.course_id.is_some() && c.id == t.course_id).cloned(),
            overdue: t.is_overdue(day), task: t,
        }).collect();
        Ok(DayAgenda { day, is_today: day == today, items, tasks: task_items, journal: self.journal.get(day)?,
                       has_courses: !courses.is_empty() })
    }

    pub fn save_journal(&self, day: NaiveDate, body: &str) -> AppResult {
        self.journal.put(day, body)?;
        self.bus.publish(Topic::Journal);
        Ok(())
    }
}

// ---- events and routines --------------------------------------------------------------------------------------------

use crate::domain::model::all_day_time;

#[derive(Clone, Debug)]
pub struct EventInput {
    pub day: NaiveDate,
    pub start: i32,
    pub end: i32,
    pub title: String,
    pub details: String,
    pub color: String,
    pub all_day: bool,
    pub milestone: bool,
}

#[derive(Clone, Debug)]
pub struct RoutineInput {
    pub title: String,
    pub minute: i32,
    pub weekdays: Vec<u8>,
    pub color: String,
}

impl PlannerService {
    pub fn event(&self, id: i64) -> AppResult<Event> {
        self.events.get(id)?.ok_or_else(|| super::errors::AppError::NotFound(format!("Event {id} does not exist.")))
    }

    pub fn save_event(&self, id: Option<i64>, data: &EventInput) -> AppResult<i64> {
        let time = if data.all_day { all_day_time() } else if data.milestone { milestone_time(data.start) } else { TimeRange::new(data.start, data.end)? };
        let mut event = match id {
            None => Event::new(data.day, time, ""),
            Some(id) => self.event(id)?,
        };
        event.day = data.day;
        event.time = time;
        event.title = data.title.trim().into();
        event.details = data.details.clone();
        event.color = data.color.clone();
        event.validate()?;
        let id = match event.id {
            None => self.events.add(&event)?,
            Some(id) => {
                self.events.update(&event)?;
                id
            }
        };
        self.bus.publish(Topic::Events);
        Ok(id)
    }

    pub fn delete_event(&self, id: i64) -> AppResult {
        self.events.delete(id)?;
        self.bus.publish(Topic::Events);
        Ok(())
    }

    pub fn routines(&self) -> AppResult<Vec<Routine>> {
        self.routines.list()
    }

    pub fn save_routine(&self, id: Option<i64>, data: &RoutineInput) -> AppResult<i64> {
        let mut days = data.weekdays.clone();
        days.sort();
        days.dedup();
        let mut routine = Routine { title: data.title.trim().into(), minute: data.minute, weekdays: days, color: data.color.clone(), id };
        routine.validate()?;
        let id = match id {
            None => self.routines.add(&routine)?,
            Some(id) => {
                if !self.routines.list()?.iter().any(|r| r.id == Some(id)) {
                    return Err(super::errors::AppError::NotFound(format!("Routine {id} does not exist.")));
                }
                self.routines.update(&routine)?;
                id
            }
        };
        routine.id = Some(id);
        self.bus.publish(Topic::Events);
        Ok(id)
    }

    pub fn delete_routine(&self, id: i64) -> AppResult {
        self.routines.delete(id)?;
        self.bus.publish(Topic::Events);
        Ok(())
    }

    pub fn journal_of(&self, day: NaiveDate) -> AppResult<String> {
        self.journal.get(day)
    }
}
