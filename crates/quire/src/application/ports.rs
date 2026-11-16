//! What the use cases need from the outside world, as plain types and traits.

use serde_json::Value;

/// One record travelling between devices: plain JSON that names related records by uid, never by local row id.
/// `modified` is UTC with milliseconds ("2026-11-09T01:09:41.214Z"), so text order is time order.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncRecord {
    pub kind: String,
    pub uid: String,
    pub modified: String,
    pub deleted: bool,
    pub data: Option<Value>,
}

impl SyncRecord {
    pub fn key(&self) -> (String, String) {
        (self.kind.clone(), self.uid.clone())
    }
}

use chrono::{NaiveDate, NaiveDateTime};

use super::errors::AppResult;
use crate::domain::ideas::Idea;
use crate::domain::model::{Course, Task};

/// Now and today, swappable in tests.
pub trait Clock {
    fn now(&self) -> NaiveDateTime;
    fn today(&self) -> NaiveDate {
        self.now().date()
    }
}

/// Small persistent settings, e.g. when the last sync happened.
pub trait KeyValueStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: Option<&str>) -> AppResult;
}

pub trait CourseRepository {
    fn list(&self) -> AppResult<Vec<Course>>;
}

pub trait TaskRepository {
    fn list(&self, include_done: bool, course_id: Option<i64>) -> AppResult<Vec<Task>>;
    fn get(&self, id: i64) -> AppResult<Option<Task>>;
    fn add(&self, task: &Task) -> AppResult<i64>;
    fn update(&self, task: &Task) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
}

pub trait IdeaRepository {
    fn list(&self) -> AppResult<Vec<Idea>>;
    fn get(&self, id: i64) -> AppResult<Option<Idea>>;
    fn add(&self, idea: &Idea) -> AppResult<i64>;
    fn update(&self, idea: &Idea) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
}
