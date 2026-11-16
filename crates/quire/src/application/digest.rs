//! The morning and evening digest (port of `services/digest.py`): once each a day, or by hand any time.

use std::rc::Rc;

use chrono::{Duration, NaiveDate, Timelike};

use super::errors::AppResult;
use super::ideas::IdeaService;
use super::planner::PlannerService;
use super::ports::{Clock, KeyValueStore};
use super::tasks::TaskService;

const PREFIX: &str = "digest.";
pub const MORNING: u32 = 7 * 60 + 30;
pub const EVENING: u32 = 21 * 60 + 30;
pub const SWITCH_HOUR: u32 = 15; // by-hand digest: today's before, tomorrow's after (same as the phone)

/// The morning digest (today) or the evening one (tomorrow, and today's ideas).
#[derive(Clone, Debug, PartialEq)]
pub struct Digest {
    pub morning: bool,
    /// (start minute, title) of the day it's about, at most 6.
    pub items: Vec<(i32, String)>,
    /// Tasks due that day, at most 5.
    pub due: Vec<String>,
    pub overdue: usize,
    /// Caught today (evening) or since yesterday (morning), still open; at most 5.
    pub ideas: Vec<String>,
    /// An older open idea, now and then, so it isn't lost.
    pub idea_back: String,
}

pub struct DigestService {
    pub planner: Rc<PlannerService>,
    pub tasks: Rc<TaskService>,
    pub ideas: Rc<IdeaService>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
}

impl DigestService {
    pub fn enabled(&self) -> bool {
        self.settings.get(&format!("{PREFIX}on")).as_deref() != Some("0")
    }

    pub fn set_enabled(&self, on: bool) -> AppResult {
        self.settings.set(&format!("{PREFIX}on"), Some(if on { "1" } else { "0" }))
    }

    /// The digest to show now, if one is due and wasn't shown today; marks it shown.
    pub fn due_now(&self) -> AppResult<Option<Digest>> {
        if !self.enabled() {
            return Ok(None);
        }
        let now = self.clock.now();
        let (minute, today) = (now.hour() * 60 + now.minute(), now.date());
        for (morning, at, key) in [(false, EVENING, "evening"), (true, MORNING, "morning")] {
            let shown = self.settings.get(&format!("{PREFIX}{key}"));
            if minute >= at && shown.as_deref() != Some(today.to_string().as_str()) {
                if morning && minute >= EVENING {
                    continue; // too late for the morning one
                }
                self.settings.set(&format!("{PREFIX}{key}"), Some(&today.to_string()))?;
                return self.build(morning, today, true).map(Some);
            }
        }
        Ok(None)
    }

    /// The digest for right now, asked for by hand: today's until 15:00, then tomorrow's. Doesn't mark it shown.
    pub fn current(&self) -> AppResult<Digest> {
        let now = self.clock.now();
        self.build(now.hour() < SWITCH_HOUR, now.date(), false)
    }

    pub fn build(&self, morning: bool, today: NaiveDate, mark: bool) -> AppResult<Digest> {
        let day = if morning { today } else { today + Duration::days(1) };
        let agenda = self.planner.day_agenda(day)?;
        let mut items: Vec<(i32, String)> = agenda.items.iter().map(|i| (i.time.start, i.title.clone())).collect();
        items.sort();
        items.truncate(6);
        let due: Vec<String> = agenda.tasks.iter().filter(|t| !t.task.done).map(|t| t.task.title.clone()).take(5).collect();
        let overdue = self.tasks.groups(false, None)?.iter().flat_map(|g| g.items.iter()).filter(|t| t.overdue).count();
        let since = if morning { today - Duration::days(1) } else { today };
        let ideas = self.ideas.caught_between(since, today)?.into_iter().filter(|i| !i.done).map(|i| i.text).take(5).collect();
        let back = self.ideas.resurfaced(mark)?;
        Ok(Digest { morning, items, due, overdue, ideas,
                    idea_back: back.map(|i| i.text.lines().next().unwrap_or("").to_string()).unwrap_or_default() })
    }
}
