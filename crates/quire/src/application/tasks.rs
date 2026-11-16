//! Use cases for tasks, homework, assignments and exams (port of `services/tasks.py`).

use std::rc::Rc;

use chrono::NaiveDate;

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::{Clock, CourseRepository, TaskRepository};
use crate::domain::model::{Course, DueBucket, Task, TaskKind};

/// What a form hands in (port of `inputs.TaskInput`).
#[derive(Clone, Debug)]
pub struct TaskInput {
    pub title: String,
    pub kind: TaskKind,
    pub course_id: Option<i64>,
    pub due: Option<NaiveDate>,
    pub details: String,
    pub done: bool,
}

impl TaskInput {
    pub fn titled(title: &str) -> Self {
        Self { title: title.into(), kind: TaskKind::Task, course_id: None, due: None, details: String::new(), done: false }
    }
}

#[derive(Clone, Debug)]
pub struct TaskItem {
    pub task: Task,
    pub course: Option<Course>,
    pub overdue: bool,
}

#[derive(Clone, Debug)]
pub struct TaskGroup {
    pub bucket: DueBucket,
    pub items: Vec<TaskItem>,
}

pub struct TaskService {
    tasks: Rc<dyn TaskRepository>,
    courses: Rc<dyn CourseRepository>,
    clock: Rc<dyn Clock>,
    bus: Rc<ChangeBus>,
}

impl TaskService {
    pub fn new(tasks: Rc<dyn TaskRepository>, courses: Rc<dyn CourseRepository>, clock: Rc<dyn Clock>, bus: Rc<ChangeBus>) -> Self {
        Self { tasks, courses, clock, bus }
    }

    /// Tasks grouped by when they are due; empty groups are left out.
    pub fn groups(&self, include_done: bool, course_id: Option<i64>) -> AppResult<Vec<TaskGroup>> {
        let today = self.clock.today();
        let courses = self.courses.list()?;
        let mut groups: Vec<TaskGroup> = Vec::new();
        for t in self.tasks.list(include_done, course_id)? {
            let bucket = t.bucket(today);
            let item = TaskItem { course: courses.iter().find(|c| c.id == t.course_id && t.course_id.is_some()).cloned(),
                                  overdue: t.is_overdue(today), task: t };
            match groups.iter_mut().find(|g| g.bucket == bucket) {
                Some(g) => g.items.push(item),
                None => groups.push(TaskGroup { bucket, items: vec![item] }),
            }
        }
        groups.sort_by_key(|g| g.bucket);
        Ok(groups)
    }

    fn get(&self, id: i64) -> AppResult<Task> {
        self.tasks.get(id)?.ok_or_else(|| AppError::NotFound(format!("Task {id} does not exist.")))
    }

    pub fn task(&self, id: i64) -> AppResult<Task> {
        self.get(id)
    }

    /// Create a task, or change one. Only the input's fields change, so an imported task keeps its register link.
    pub fn save(&self, id: Option<i64>, data: &TaskInput) -> AppResult<i64> {
        let mut task = match id {
            None => Task::new(""),
            Some(id) => self.get(id)?,
        };
        task.title = data.title.trim().to_string();
        task.kind = data.kind;
        task.course_id = data.course_id;
        task.due = data.due;
        task.details = data.details.clone();
        task.done = data.done;
        task.validate()?;
        let id = match task.id {
            None => self.tasks.add(&task)?,
            Some(id) => {
                self.tasks.update(&task)?;
                id
            }
        };
        self.bus.publish(Topic::Tasks);
        Ok(id)
    }

    pub fn set_done(&self, id: i64, done: bool) -> AppResult {
        let mut task = self.get(id)?;
        task.done = done;
        self.tasks.update(&task)?;
        self.bus.publish(Topic::Tasks);
        Ok(())
    }

    /// The one task to do next (never a list): oldest overdue, else one due today (by title), else the soonest due.
    pub fn next_step(&self) -> AppResult<Option<Task>> {
        let today = self.clock.today();
        let open: Vec<Task> = self.tasks.list(false, None)?.into_iter().filter(|t| t.due.is_some()).collect();
        let pick = open.iter().filter(|t| t.due.unwrap() < today).min_by_key(|t| t.due)
            .or_else(|| open.iter().filter(|t| t.due == Some(today)).min_by_key(|t| t.title.to_lowercase()))
            .or_else(|| open.iter().filter(|t| t.due.unwrap() > today).min_by_key(|t| t.due));
        Ok(pick.cloned())
    }

    pub fn delete(&self, id: i64) -> AppResult {
        self.tasks.delete(id)?;
        self.bus.publish(Topic::Tasks);
        Ok(())
    }
}
