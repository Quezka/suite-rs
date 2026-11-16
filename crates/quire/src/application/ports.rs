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
    fn add(&self, course: &Course) -> AppResult<i64>;
    fn update(&self, course: &Course) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
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

use std::collections::HashSet;

use crate::domain::model::{Event, Job, Routine, Shift};

pub trait EventRepository {
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Event>>;
    fn get(&self, id: i64) -> AppResult<Option<Event>>;
    fn add(&self, event: &Event) -> AppResult<i64>;
    fn update(&self, event: &Event) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
}

pub trait RoutineRepository {
    fn list(&self) -> AppResult<Vec<Routine>>;
    fn add(&self, routine: &Routine) -> AppResult<i64>;
    fn update(&self, routine: &Routine) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
}

pub trait JournalRepository {
    fn get(&self, day: NaiveDate) -> AppResult<String>;
    fn put(&self, day: NaiveDate, body: &str) -> AppResult;
}

pub trait JobRepository {
    fn list(&self) -> AppResult<Vec<Job>>;
}

pub trait ShiftRepository {
    fn starting_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Shift>>;
    /// Weekly-schedule occurrences the user skipped: (job id, day, start minute).
    fn skipped(&self, first: NaiveDate, last: NaiveDate) -> AppResult<HashSet<(i64, NaiveDate, i32)>>;
}

use crate::domain::model::{Note, NoteImage, Notebook};

pub trait NoteRepository {
    /// Pinned first, then most recently edited. `text` matches body or topic.
    fn search(&self, text: &str, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<Note>>;
    fn find_by_title(&self, title: &str, course_id: Option<i64>) -> AppResult<Option<Note>>;
    fn get(&self, id: i64) -> AppResult<Option<Note>>;
    fn topics(&self, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<String>>;
    fn rename_topic(&self, course_id: Option<i64>, old: &str, new: &str, notebook_id: Option<i64>) -> AppResult<usize>;
    fn add(&self, note: &Note) -> AppResult<i64>;
    fn update(&self, note: &Note) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
}

pub trait NotebookRepository {
    fn list(&self) -> AppResult<Vec<Notebook>>;
    fn get(&self, id: i64) -> AppResult<Option<Notebook>>;
    fn add(&self, book: &Notebook) -> AppResult<i64>;
    fn update(&self, book: &Notebook) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
    fn note_count(&self, id: i64) -> AppResult<usize>;
}

pub trait ImageRepository {
    fn get(&self, uid: &str) -> AppResult<Option<NoteImage>>;
    fn add(&self, image: &NoteImage) -> AppResult<i64>;
    fn replace(&self, uid: &str, mime: &str, data: &[u8]) -> AppResult;
    /// A new random id (32 hex digits, like Python's uuid4().hex).
    fn new_uid(&self) -> String;
}

pub trait CourseLookup {
    fn get(&self, id: i64) -> AppResult<Option<Course>>;
}
