//! The Pomodoro focus timer and focus blocks (port of `services/focus.py`). Holds the one running timer; the UI calls
//! `tick()` about once a second. Sessions are logged with the task (or project) you were on; one cut short counts too.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDateTime, Timelike};

use super::bus::{ChangeBus, Topic};
use super::errors::AppResult;
use super::ports::{Clock, CourseRepository, EventRepository, FocusLogRepository, KeyValueStore, TaskRepository};
use super::tasks::TaskItem;
use crate::domain::focus::{plan_block, FocusSession, FocusSettings, Phase, PomodoroTimer};
use crate::domain::model::{Event, Task, TimeRange};

const PREFIX: &str = "focus.";
pub const BLOCK_COLOR: &str = "#4f46e5";

#[derive(Clone, Debug, PartialEq)]
pub struct FocusState {
    pub phase: Phase,
    pub remaining_seconds: f64,
    pub progress: f64,
    pub running: bool,
    pub started: bool,
    pub round: u32,
    pub rounds: u32,
    pub completed: u32,
    pub block_left_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FocusStats {
    pub today_sessions: usize,
    pub today_minutes: i64,
    pub week_sessions: usize,
    pub week_minutes: i64,
    /// (task or project, minutes), most first.
    pub today_by_project: Vec<(String, i64)>,
}

/// What `tick` reports: the phase that ended, what comes next, and whether a block finished.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusEvent {
    pub phase: Phase,
    pub next_phase: Phase,
    pub block_done: bool,
}

pub struct FocusService {
    pub log: Rc<dyn FocusLogRepository>,
    pub settings: Rc<dyn KeyValueStore>,
    pub tasks: Rc<dyn TaskRepository>,
    pub courses: Rc<dyn CourseRepository>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
    pub events: Option<Rc<dyn EventRepository>>,
    timer: RefCell<PomodoroTimer>,
}

impl FocusService {
    pub fn new(log: Rc<dyn FocusLogRepository>, settings: Rc<dyn KeyValueStore>, tasks: Rc<dyn TaskRepository>,
               courses: Rc<dyn CourseRepository>, clock: Rc<dyn Clock>, bus: Rc<ChangeBus>, events: Option<Rc<dyn EventRepository>>) -> Self {
        let mut s = Self { log, settings, tasks, courses, clock, bus, events, timer: RefCell::new(PomodoroTimer::new(FocusSettings::default())) };
        s.timer = RefCell::new(PomodoroTimer::new(s.load_settings()));
        s
    }

    fn key(name: &str) -> String {
        format!("{PREFIX}{name}")
    }

    fn load_settings(&self) -> FocusSettings {
        let d = FocusSettings::default();
        let number = |n: &str, def: i64| self.settings.get(&Self::key(n)).filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse().ok()).unwrap_or(def);
        let auto = self.settings.get(&Self::key("auto_continue"));
        FocusSettings {
            work_minutes: number("work", d.work_minutes), short_break_minutes: number("short_break", d.short_break_minutes),
            long_break_minutes: number("long_break", d.long_break_minutes), rounds: number("rounds", d.rounds as i64) as u32,
            auto_continue: auto.map_or(d.auto_continue, |a| a == "1"),
        }
    }

    pub fn settings_data(&self) -> FocusSettings {
        self.load_settings()
    }

    pub fn save_settings(&self, s: &FocusSettings) -> AppResult {
        s.validate()?;
        for (n, v) in [("work", s.work_minutes), ("short_break", s.short_break_minutes), ("long_break", s.long_break_minutes),
                       ("rounds", s.rounds as i64)] {
            self.settings.set(&Self::key(n), Some(&v.to_string()))?;
        }
        self.settings.set(&Self::key("auto_continue"), Some(if s.auto_continue { "1" } else { "0" }))?;
        self.timer.borrow_mut().apply_settings(s.clone())?;
        self.bus.publish(Topic::Focus);
        Ok(())
    }

    // ---- what you're focusing on ----

    fn focus_task_inner(&self) -> AppResult<Option<Task>> {
        let id = self.settings.get(&Self::key("task")).and_then(|v| v.parse::<i64>().ok());
        Ok(match id { Some(id) => self.tasks.get(id)?.filter(|t| !t.done), None => None })
    }

    pub fn focus_task(&self) -> AppResult<Option<Task>> {
        self.focus_task_inner()
    }

    pub fn set_focus_task(&self, id: Option<i64>) -> AppResult {
        self.settings.set(&Self::key("task"), id.map(|i| i.to_string()).as_deref())?;
        self.settings.set(&Self::key("project"), None)
    }

    /// The ad-hoc project you're on, when it isn't one of your tasks ("" for none).
    pub fn focus_project(&self) -> String {
        self.settings.get(&Self::key("project")).unwrap_or_default()
    }

    pub fn set_focus_project(&self, name: &str) -> AppResult {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        self.settings.set(&Self::key("project"), if name.is_empty() { None } else { Some(&name) })?;
        self.settings.set(&Self::key("task"), None)
    }

    pub fn recent_projects(&self, limit: usize) -> AppResult<Vec<String>> {
        self.log.recent_labels(limit)
    }

    /// Open tasks worth focusing on: overdue, due soon, or without a date.
    pub fn candidate_tasks(&self, days: i64) -> AppResult<Vec<TaskItem>> {
        let today = self.clock.today();
        let courses = self.courses.list()?;
        let mut tasks: Vec<Task> = self.tasks.list(false, None)?.into_iter()
            .filter(|t| t.due.is_none_or(|d| (d - today).num_days() <= days)).collect();
        tasks.sort_by(|a, b| (a.due.is_none(), a.due.unwrap_or(today), a.title.to_lowercase())
            .cmp(&(b.due.is_none(), b.due.unwrap_or(today), b.title.to_lowercase())));
        Ok(tasks.into_iter().map(|t| TaskItem { course: courses.iter().find(|c| t.course_id.is_some() && c.id == t.course_id).cloned(),
                                                 overdue: t.is_overdue(today), task: t }).collect())
    }

    // ---- controls ----

    pub fn state(&self) -> FocusState {
        let timer = self.timer.borrow();
        let now = self.clock.now();
        FocusState {
            phase: timer.phase, remaining_seconds: timer.remaining(now).num_milliseconds() as f64 / 1000.0,
            progress: timer.progress(now), running: timer.running(), started: timer.started(), round: timer.round(),
            rounds: timer.settings.rounds, completed: timer.completed,
            block_left_seconds: timer.block_ends.map(|e| ((e - now).num_milliseconds() as f64 / 1000.0).max(0.0)),
        }
    }

    pub fn block_plan(&self, total_minutes: i64) -> AppResult<Vec<(Phase, i64)>> {
        Ok(plan_block(total_minutes, &self.timer.borrow().settings)?)
    }

    /// Focus for a set time: rounds and breaks run by themselves; at its end the block goes into the week as an event.
    pub fn start_block(&self, total_minutes: i64) -> AppResult {
        plan_block(total_minutes, &self.timer.borrow().settings)?;
        self.log_cut_short()?;
        self.end_block(None)?;
        self.timer.borrow_mut().start_block(self.clock.now(), total_minutes)?;
        self.bus.publish(Topic::Focus);
        Ok(())
    }

    fn end_block(&self, until: Option<NaiveDateTime>) -> AppResult {
        let started = self.timer.borrow().block_started;
        let Some(started) = started else { return Ok(()) };
        {
            let mut t = self.timer.borrow_mut();
            t.block_started = None;
            t.block_ends = None;
        }
        self.add_block_event(started, until.unwrap_or(self.clock.now()))
    }

    fn add_block_event(&self, started: NaiveDateTime, end: NaiveDateTime) -> AppResult {
        let Some(events) = &self.events else { return Ok(()) };
        if end - started < Duration::minutes(1) {
            return Ok(());
        }
        let first = (started.hour() * 60 + started.minute()) as i32;
        let last = if end.date() == started.date() { (end.hour() * 60 + end.minute()) as i32 } else { 24 * 60 };
        if last <= first {
            return Ok(());
        }
        let what = match self.focus_task_inner()? { Some(t) => t.title, None => self.focus_project() };
        let title = if what.is_empty() { "Focus".to_string() } else { format!("Focus: {what}") };
        events.add(&Event { details: "Quire focus block".into(), color: BLOCK_COLOR.into(),
                            ..Event::new(started.date(), TimeRange { start: first, end: last }, &title) })?;
        self.bus.publish(Topic::Events);
        Ok(())
    }

    pub fn toggle(&self) {
        let now = self.clock.now();
        let mut t = self.timer.borrow_mut();
        if t.running() { t.pause(now) } else { t.start(now) }
    }

    pub fn reset(&self) -> AppResult {
        self.log_cut_short()?;
        self.end_block(None)?;
        self.timer.borrow_mut().reset();
        Ok(())
    }

    pub fn skip(&self) -> AppResult {
        self.log_cut_short()?;
        self.timer.borrow_mut().skip(self.clock.now());
        Ok(())
    }

    /// Quire is quitting: the timer can't carry on, so the focus so far counts.
    pub fn stop(&self) -> AppResult {
        self.reset()
    }

    /// A focus session ended early still counts towards your focus time.
    fn log_cut_short(&self) -> AppResult {
        self.tick()?; // a phase that already ran out counts as finished, not cut short
        let session = self.timer.borrow().cut_short(self.clock.now());
        if let Some(s) = session {
            self.log_session(s.started, s.minutes, false)?;
        }
        Ok(())
    }

    /// Advance the timer; returns the phase that just ended (after a long sleep, the latest of several).
    pub fn tick(&self) -> AppResult<Option<FocusEvent>> {
        let mut last = None;
        let mut block_started = self.timer.borrow().block_started;
        loop {
            let event = self.timer.borrow_mut().tick(self.clock.now());
            let Some(e) = event else { break };
            last = Some(FocusEvent { phase: e.phase, next_phase: e.next_phase, block_done: e.block_done });
            if e.phase == Phase::Work {
                self.log_session(e.started, e.minutes, true)?;
            }
            if e.block_done {
                if let Some(start) = block_started.take() {
                    self.add_block_event(start, e.started + Duration::minutes(e.minutes))?;
                }
            }
        }
        Ok(last)
    }

    fn log_session(&self, started: NaiveDateTime, minutes: i64, complete: bool) -> AppResult {
        let task = self.focus_task_inner()?;
        self.log.add(&FocusSession { started, minutes, task_id: task.as_ref().and_then(|t| t.id),
                                     label: task.map_or_else(|| self.focus_project(), |t| t.title), complete })?;
        self.bus.publish(Topic::Focus);
        Ok(())
    }

    // ---- stats ----

    pub fn stats(&self) -> AppResult<FocusStats> {
        let today = self.clock.today();
        let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
        let week = self.log.between(monday, monday + Duration::days(6))?;
        let todays: Vec<&FocusSession> = week.iter().filter(|s| s.started.date() == today).collect();
        let mut projects: HashMap<String, i64> = HashMap::new();
        for s in &todays {
            if !s.label.is_empty() {
                *projects.entry(s.label.clone()).or_default() += s.minutes;
            }
        }
        let mut by: Vec<(String, i64)> = projects.into_iter().collect();
        by.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(FocusStats {
            today_sessions: todays.iter().filter(|s| s.complete).count(), today_minutes: todays.iter().map(|s| s.minutes).sum(),
            week_sessions: week.iter().filter(|s| s.complete).count(), week_minutes: week.iter().map(|s| s.minutes).sum(),
            today_by_project: by,
        })
    }
}
