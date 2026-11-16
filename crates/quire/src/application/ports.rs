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
    fn add(&self, job: &Job) -> AppResult<i64>;
    fn update(&self, job: &Job) -> AppResult;
    /// With all of its shifts (the database cascades).
    fn delete(&self, id: i64) -> AppResult;
}

pub trait ShiftRepository {
    fn starting_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Shift>>;
    /// Weekly-schedule occurrences the user skipped: (job id, day, start minute).
    fn skipped(&self, first: NaiveDate, last: NaiveDate) -> AppResult<HashSet<(i64, NaiveDate, i32)>>;
    fn get(&self, id: i64) -> AppResult<Option<Shift>>;
    fn add(&self, shift: &Shift) -> AppResult<i64>;
    fn update(&self, shift: &Shift) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
    /// Take one occurrence of a weekly shift off the schedule.
    fn skip(&self, job_id: i64, day: NaiveDate, start: i32) -> AppResult;
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

use crate::domain::water::WaterLog;

pub trait WaterRepository {
    /// Oldest first (day, minute, id).
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<WaterLog>>;
    fn add(&self, log: &WaterLog) -> AppResult<i64>;
    fn delete(&self, id: i64) -> AppResult;
}

use crate::domain::medicine::{DoseLog, Medicine};

pub trait MedicineRepository {
    fn all(&self) -> AppResult<Vec<Medicine>>;
    fn get(&self, id: i64) -> AppResult<Option<Medicine>>;
    fn add(&self, m: &Medicine) -> AppResult<i64>;
    fn update(&self, m: &Medicine) -> AppResult;
    fn delete(&self, id: i64) -> AppResult;
    fn logs_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<DoseLog>>;
    fn add_log(&self, log: &DoseLog) -> AppResult<i64>;
    fn delete_log(&self, id: i64) -> AppResult;
}

use crate::domain::focus::FocusSession;
use crate::domain::sleep::SleepLog;

pub trait SleepRepository {
    fn get(&self, day: NaiveDate) -> AppResult<Option<SleepLog>>;
    /// Oldest first.
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<SleepLog>>;
    /// One night per day: replaces that day's.
    fn put(&self, log: &SleepLog) -> AppResult;
    fn delete(&self, day: NaiveDate) -> AppResult;
}

pub trait FocusLogRepository {
    fn add(&self, session: &FocusSession) -> AppResult;
    /// Sessions started on days first..=last, oldest first.
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<FocusSession>>;
    /// Labels of sessions without a task, newest first, distinct.
    fn recent_labels(&self, limit: usize) -> AppResult<Vec<String>>;
}

pub const FIREBASE: &str = "firebase";
pub const SERVER: &str = "server";

/// Where to sync. For Firebase, `project_id` and `api_key` are the project's; for a Quire server, `project_id` is the
/// server's address and `api_key` is unused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudConfig {
    pub project_id: String,
    pub api_key: String,
    pub backend: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudSession {
    pub user_id: String,
    pub email: String,
    /// Short-lived.
    pub token: String,
    /// Long-lived; kept in the keyring.
    pub refresh_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Somewhere safe to keep a password (the OS keyring).
pub trait CredentialStore {
    fn load(&self) -> Option<Credentials>;
    fn save(&self, c: &Credentials) -> AppResult;
    fn clear(&self) -> AppResult;
}

/// This device's side of sync.
pub trait SyncStorePort {
    fn outgoing(&self) -> AppResult<Vec<SyncRecord>>;
    /// Takes in records newer than ours; returns the (kind, uid) applied.
    fn apply(&self, records: &[SyncRecord]) -> AppResult<HashSet<(String, String)>>;
    fn mark_sent(&self, records: &[SyncRecord]) -> AppResult;
    fn pending(&self) -> AppResult<i64>;
    fn clear_all(&self) -> AppResult;
    fn mark_all_dirty(&self) -> AppResult;
}

/// A cloud database with accounts (Firebase, or a Quire server). Network only: used on a worker thread.
pub trait CloudBackend: Send + Sync {
    fn sign_up(&self, config: &CloudConfig, email: &str, password: &str, invite: &str) -> AppResult<CloudSession>;
    fn sign_in(&self, config: &CloudConfig, email: &str, password: &str) -> AppResult<CloudSession>;
    fn refresh(&self, config: &CloudConfig, refresh_token: &str) -> AppResult<CloudSession>;
    /// Records changed since `cursor`, and the new cursor.
    fn pull(&self, config: &CloudConfig, session: &CloudSession, cursor: Option<&str>) -> AppResult<(Vec<SyncRecord>, Option<String>)>;
    fn push(&self, config: &CloudConfig, session: &CloudSession, records: &[SyncRecord]) -> AppResult;
}

// ---- the school register ----------------------------------------------------------------------------------------

use crate::domain::model::{Grade, Lesson, Subject, TaskKind};
use crate::domain::register::{Absence, Book, Notice, SchoolDocument};

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteSubject {
    pub id: String,
    pub name: String,
    pub teachers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteAssignment {
    pub id: String,
    pub day: NaiveDate,
    pub kind: TaskKind,
    pub text: String,
    pub subject_id: Option<String>,
    pub subject_name: String,
    pub author: String,
    /// "agenda" is complete for the dates asked for (missing = deleted); "homework" lists only current items.
    pub feed: String,
    /// Already marked done on the register.
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteGrade {
    pub id: String,
    pub day: NaiveDate,
    pub subject_id: Option<String>,
    pub subject_name: String,
    pub display: String,
    pub value: Option<f64>,
    pub component: String,
    pub period: String,
    pub notes: String,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteLesson {
    pub id: String,
    pub day: NaiveDate,
    pub subject_id: Option<String>,
    pub subject_name: String,
    pub topic: String,
    pub teacher: String,
    pub hour: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegisterAccount {
    pub student_name: String,
}

/// Everything fetched in one sync. A part that couldn't be fetched is None (not empty), so applying leaves the local
/// copy of that part alone; `problems` says what went wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct RegisterSnapshot {
    pub account: RegisterAccount,
    pub subjects: Option<Vec<RemoteSubject>>,
    pub assignments: Option<Vec<RemoteAssignment>>,
    pub homework: Option<Vec<RemoteAssignment>>,
    pub grades: Option<Vec<RemoteGrade>>,
    pub lessons: Option<Vec<RemoteLesson>>,
    pub assignment_window: Option<(NaiveDate, NaiveDate)>,
    pub lesson_window: Option<(NaiveDate, NaiveDate)>,
    pub absences: Option<Vec<Absence>>,
    pub notices: Option<Vec<Notice>>,
    pub books: Option<Vec<Book>>,
    pub documents: Option<Vec<SchoolDocument>>,
    pub school_days: Option<Vec<NaiveDate>>,
    pub problems: Vec<String>,
}

/// Why the register said no.
#[derive(Clone, Debug, PartialEq)]
pub enum RegisterError {
    /// Wrong username or password: stop the whole sync.
    Authentication(String),
    /// One part failed (network, an endpoint changed): the rest carries on.
    Failed(String),
}

pub type RegisterResult<T> = Result<T, RegisterError>;

/// An online school register (Classeviva). Network only: used on a worker thread.
pub trait SchoolRegister: Send + Sync {
    /// Shown to the user, e.g. "Classeviva".
    fn name(&self) -> String;
    fn login(&self, c: &Credentials) -> RegisterResult<RegisterAccount>;
    fn subjects(&self) -> RegisterResult<Vec<RemoteSubject>>;
    fn assignments(&self, first: NaiveDate, last: NaiveDate) -> RegisterResult<Vec<RemoteAssignment>>;
    fn homework(&self) -> RegisterResult<Vec<RemoteAssignment>>;
    fn grades(&self) -> RegisterResult<Vec<RemoteGrade>>;
    fn lessons(&self, first: NaiveDate, last: NaiveDate) -> RegisterResult<Vec<RemoteLesson>>;
    fn absences(&self) -> RegisterResult<Vec<Absence>>;
    fn notices(&self) -> RegisterResult<Vec<Notice>>;
    /// Opens it (the register then counts it read); its text, "" when it's all in the attachments.
    fn open_notice(&self, n: &Notice) -> RegisterResult<String>;
    fn notice_attachment(&self, n: &Notice, number: i32) -> RegisterResult<Vec<u8>>;
    fn books(&self) -> RegisterResult<Vec<Book>>;
    fn documents(&self) -> RegisterResult<Vec<SchoolDocument>>;
    fn document_file(&self, d: &SchoolDocument) -> RegisterResult<Vec<u8>>;
    fn school_days(&self) -> RegisterResult<Vec<NaiveDate>>;
}

/// The local mirror of the register.
pub trait SchoolRecordRepository {
    fn grades(&self) -> AppResult<Vec<Grade>>;
    fn replace_grades(&self, grades: &[Grade]) -> AppResult;
    fn lessons_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Lesson>>;
    fn replace_lessons(&self, first: NaiveDate, last: NaiveDate, lessons: &[Lesson]) -> AppResult;
    fn subjects(&self) -> AppResult<Vec<Subject>>;
    fn replace_subjects(&self, subjects: &[Subject]) -> AppResult;
    fn absences(&self) -> AppResult<Vec<Absence>>;
    fn replace_absences(&self, absences: &[Absence]) -> AppResult;
    fn notices(&self) -> AppResult<Vec<Notice>>;
    fn replace_notices(&self, notices: &[Notice]) -> AppResult;
    fn set_notice_read(&self, external_id: &str) -> AppResult;
    fn books(&self) -> AppResult<Vec<Book>>;
    fn replace_books(&self, books: &[Book]) -> AppResult;
    fn documents(&self) -> AppResult<Vec<SchoolDocument>>;
    fn replace_documents(&self, documents: &[SchoolDocument]) -> AppResult;
    fn school_days(&self) -> AppResult<Vec<NaiveDate>>;
    fn replace_school_days(&self, days: &[NaiveDate]) -> AppResult;
}

/// What the school sync needs beyond plain task and course storage.
pub trait SchoolLinks {
    /// Tasks whose external id starts with `prefix`.
    fn tasks_by_external_prefix(&self, prefix: &str) -> AppResult<Vec<Task>>;
    /// Fold one course into another: its tasks, notes, grades and lessons move over; the source is deleted.
    fn merge_courses(&self, source: i64, target: i64) -> AppResult;
}

/// The computer's own calendar app.
pub trait SystemCalendar {
    /// Whether the system keeps following the file, rather than importing it once.
    fn can_subscribe(&self) -> bool;
    fn publish(&self, text: &str) -> AppResult;
    fn connect(&self) -> AppResult;
    fn disconnect(&self) -> AppResult;
    fn open(&self) -> AppResult;
}
