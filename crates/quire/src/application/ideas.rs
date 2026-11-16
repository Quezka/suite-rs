//! Catch an idea in a second; find it again; act on it (port of `services/ideas.py`).

use std::collections::HashMap;
use std::rc::Rc;

use chrono::{NaiveDate, Timelike};

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::{Clock, IdeaRepository, KeyValueStore};
use super::tasks::{TaskInput, TaskService};
use crate::domain::ideas::{self, first_line, Idea};

const SEEN: &str = "ideas.seen"; // per device: when each idea last came back in the digest

pub struct IdeaService {
    repo: Rc<dyn IdeaRepository>,
    clock: Rc<dyn Clock>,
    bus: Rc<ChangeBus>,
    tasks: Rc<TaskService>,
    settings: Rc<dyn KeyValueStore>,
}

impl IdeaService {
    pub fn new(repo: Rc<dyn IdeaRepository>, clock: Rc<dyn Clock>, bus: Rc<ChangeBus>, tasks: Rc<TaskService>,
               settings: Rc<dyn KeyValueStore>) -> Self {
        Self { repo, clock, bus, tasks, settings }
    }

    /// Newest first; the open ones before the done ones. `query`: words and #tags.
    pub fn ideas(&self, include_done: bool, query: &str) -> AppResult<Vec<Idea>> {
        let mut items: Vec<Idea> = self.repo.list()?.into_iter()
            .filter(|i| (include_done || !i.done) && ideas::matches(i, query)).collect();
        items.sort_by(|a, b| a.done.cmp(&b.done).then(b.created.cmp(&a.created)));
        Ok(items)
    }

    pub fn add(&self, text: &str) -> AppResult<i64> {
        let text = if text.contains('\n') { text.trim().to_string() } else { text.split_whitespace().collect::<Vec<_>>().join(" ") };
        let mut idea = Idea { text, created: self.clock.now().with_nanosecond(0).unwrap(), done: false, id: None };
        idea.validate()?;
        idea.id = Some(self.repo.add(&idea)?);
        self.bus.publish(Topic::Ideas);
        Ok(idea.id.unwrap())
    }

    fn get(&self, id: i64) -> AppResult<Idea> {
        self.repo.get(id)?.ok_or_else(|| AppError::NotFound("That idea no longer exists.".into()))
    }

    pub fn edit(&self, id: i64, text: &str) -> AppResult {
        let mut idea = self.get(id)?;
        idea.text = text.trim().to_string();
        idea.validate()?;
        self.repo.update(&idea)?;
        self.bus.publish(Topic::Ideas);
        Ok(())
    }

    pub fn set_done(&self, id: i64, done: bool) -> AppResult {
        let mut idea = self.get(id)?;
        idea.done = done;
        self.repo.update(&idea)?;
        self.bus.publish(Topic::Ideas);
        Ok(())
    }

    pub fn delete(&self, id: i64) -> AppResult {
        self.repo.delete(id)?;
        self.bus.publish(Topic::Ideas);
        Ok(())
    }

    /// Undo a delete: same text and time, back again.
    pub fn restore(&self, idea: &Idea) -> AppResult<i64> {
        let id = self.repo.add(&Idea { id: None, ..idea.clone() })?;
        self.bus.publish(Topic::Ideas);
        Ok(id)
    }

    /// Tags of the open ideas, most used first.
    pub fn all_tags(&self) -> AppResult<Vec<String>> {
        let mut count: HashMap<String, usize> = HashMap::new();
        for i in self.repo.list()?.iter().filter(|i| !i.done) {
            for t in ideas::tags(&i.text) {
                *count.entry(t).or_default() += 1;
            }
        }
        let mut out: Vec<String> = count.keys().cloned().collect();
        out.sort_by(|a, b| count[b].cmp(&count[a]).then(a.cmp(b)));
        Ok(out)
    }

    /// Make the idea a task (title = first line, rest = details); the idea is then done.
    pub fn to_task(&self, id: i64, due: Option<NaiveDate>) -> AppResult<i64> {
        let idea = self.get(id)?;
        let mut input = TaskInput::titled(&first_line(&idea.text, 120));
        if input.title.is_empty() {
            input.title = idea.text.chars().take(120).collect();
        }
        input.due = due;
        input.details = idea.text.trim().split_once('\n').map(|(_, rest)| rest.trim().to_string()).unwrap_or_default();
        let task = self.tasks.save(None, &input)?;
        self.set_done(id, true)?;
        Ok(task)
    }

    pub fn caught_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Idea>> {
        Ok(ideas::new_on(&self.repo.list()?, first, last))
    }

    /// An older open idea worth another look (digest); `mark` remembers it came back today.
    pub fn resurfaced(&self, mark: bool) -> AppResult<Option<Idea>> {
        let mut seen: HashMap<String, String> = self.settings.get(SEEN)
            .and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
        let today = self.clock.today();
        let idea = ideas::resurface(&self.repo.list()?, today, &seen);
        if let (Some(i), true) = (&idea, mark) {
            seen.insert(i.id.unwrap_or_default().to_string(), today.to_string());
            self.settings.set(SEEN, Some(&serde_json::to_string(&seen).unwrap_or_default()))?;
        }
        Ok(idea)
    }
}
