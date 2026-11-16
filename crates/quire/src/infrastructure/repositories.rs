//! The use cases' repositories on SQLite (port of `infrastructure/repositories.py`): same SQL, same order, so both
//! apps list things alike. Writes go through the sync triggers like the Python app's.

use std::rc::Rc;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{params, OptionalExtension, Row};

use super::sqlite::Database;
use crate::application::errors::{AppError, AppResult};
use crate::application::ports::{CourseRepository, IdeaRepository, KeyValueStore, TaskRepository};
use crate::domain::ideas::Idea;
use crate::domain::model::{ClassSlot, Course, Task, TaskKind, TimeRange};

fn store(e: rusqlite::Error) -> AppError {
    AppError::Storage(e.to_string())
}

fn day(text: Option<String>) -> Option<NaiveDate> {
    text.and_then(|t| t.parse().ok())
}

pub struct SqliteSettings(pub Rc<Database>);

impl KeyValueStore for SqliteSettings {
    fn get(&self, key: &str) -> Option<String> {
        self.0.conn.query_row("SELECT value FROM settings WHERE key = ?", [key], |r| r.get(0)).optional().ok().flatten()
    }

    fn set(&self, key: &str, value: Option<&str>) -> AppResult {
        match value {
            None => self.0.conn.execute("DELETE FROM settings WHERE key = ?", [key]),
            Some(v) => self.0.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v]),
        }.map(|_| ()).map_err(store)
    }
}

pub struct SqliteCourses(pub Rc<Database>);

impl CourseRepository for SqliteCourses {
    fn list(&self) -> AppResult<Vec<Course>> {
        let conn = &self.0.conn;
        let mut stmt = conn.prepare("SELECT * FROM courses ORDER BY name COLLATE NOCASE").map_err(store)?;
        let mut courses: Vec<Course> = stmt.query_map([], |r| Ok(Course {
            name: r.get("name")?, teacher: r.get("teacher")?, room: r.get("room")?, color: r.get("color")?,
            slots: Vec::new(), id: Some(r.get("id")?), external_id: r.get("external_id")?,
        })).map_err(store)?.collect::<Result<_, _>>().map_err(store)?;
        let mut slots = conn.prepare("SELECT * FROM class_slots ORDER BY weekday, start_min").map_err(store)?;
        let rows = slots.query_map([], |r| Ok((r.get::<_, i64>("course_id")?, r.get::<_, u8>("weekday")?,
            r.get::<_, i32>("start_min")?, r.get::<_, i32>("end_min")?, r.get::<_, String>("room")?))).map_err(store)?;
        for row in rows {
            let (course, weekday, start, end, room) = row.map_err(store)?;
            if let Some(c) = courses.iter_mut().find(|c| c.id == Some(course)) {
                c.slots.push(ClassSlot { weekday, time: TimeRange { start, end }, room });
            }
        }
        Ok(courses)
    }
}

pub struct SqliteTasks(pub Rc<Database>);

const TASK_ORDER: &str = " ORDER BY due IS NULL, due, title COLLATE NOCASE";

fn task(r: &Row) -> rusqlite::Result<Task> {
    let kind: String = r.get("kind")?;
    Ok(Task {
        title: r.get("title")?,
        kind: TaskKind::from_code(&kind).unwrap_or(TaskKind::Task),
        course_id: r.get("course_id")?,
        due: day(r.get("due")?),
        done: r.get::<_, i64>("done")? != 0,
        details: r.get("details")?,
        id: Some(r.get("id")?),
        external_id: r.get("external_id")?,
    })
}

impl TaskRepository for SqliteTasks {
    fn list(&self, include_done: bool, course_id: Option<i64>) -> AppResult<Vec<Task>> {
        let mut wh: Vec<&str> = Vec::new();
        if !include_done {
            wh.push("done = 0");
        }
        if course_id.is_some() {
            wh.push("course_id = ?1");
        }
        let sql = format!("SELECT * FROM tasks{}{TASK_ORDER}", if wh.is_empty() { String::new() } else { format!(" WHERE {}", wh.join(" AND ")) });
        let mut stmt = self.0.conn.prepare(&sql).map_err(store)?;
        let rows = match course_id {
            Some(c) => stmt.query_map([c], task),
            None => stmt.query_map([], task),
        }.map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    fn get(&self, id: i64) -> AppResult<Option<Task>> {
        self.0.conn.query_row("SELECT * FROM tasks WHERE id = ?", [id], task).optional().map_err(store)
    }

    fn add(&self, t: &Task) -> AppResult<i64> {
        self.0.conn.execute(
            "INSERT INTO tasks (title, kind, course_id, due, done, details, external_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![t.title, t.kind.code(), t.course_id, t.due.map(|d| d.to_string()), t.done as i64, t.details, t.external_id],
        ).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }

    fn update(&self, t: &Task) -> AppResult {
        self.0.conn.execute(
            "UPDATE tasks SET title = ?, kind = ?, course_id = ?, due = ?, done = ?, details = ?, external_id = ? WHERE id = ?",
            params![t.title, t.kind.code(), t.course_id, t.due.map(|d| d.to_string()), t.done as i64, t.details, t.external_id, t.id],
        ).map(|_| ()).map_err(store)
    }

    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM tasks WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}

pub struct SqliteIdeas(pub Rc<Database>);

fn idea(r: &Row) -> rusqlite::Result<Idea> {
    let created: String = r.get("created")?;
    Ok(Idea {
        text: r.get("text")?,
        created: NaiveDateTime::parse_from_str(&created, "%Y-%m-%dT%H:%M:%S").unwrap_or_default(),
        done: r.get::<_, i64>("done")? != 0,
        id: Some(r.get("id")?),
    })
}

fn stamp(t: &NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string() // Python isoformat(timespec="seconds")
}

impl IdeaRepository for SqliteIdeas {
    fn list(&self) -> AppResult<Vec<Idea>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM ideas ORDER BY created DESC, id DESC").map_err(store)?;
        let rows = stmt.query_map([], idea).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    fn get(&self, id: i64) -> AppResult<Option<Idea>> {
        self.0.conn.query_row("SELECT * FROM ideas WHERE id = ?", [id], idea).optional().map_err(store)
    }

    fn add(&self, i: &Idea) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO ideas (text, created, done) VALUES (?, ?, ?)",
            params![i.text, stamp(&i.created), i.done as i64]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }

    fn update(&self, i: &Idea) -> AppResult {
        self.0.conn.execute("UPDATE ideas SET text = ?, created = ?, done = ? WHERE id = ?",
            params![i.text, stamp(&i.created), i.done as i64, i.id]).map(|_| ()).map_err(store)
    }

    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM ideas WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}

// ---- the planner's sources ----------------------------------------------------------------------------------------

use std::collections::HashSet;

use crate::application::ports::{EventRepository, JobRepository, JournalRepository, RoutineRepository, ShiftRepository};
use crate::domain::model::{Event, Job, Routine, Shift, ShiftPattern};

pub struct SqliteEvents(pub Rc<Database>);

impl EventRepository for SqliteEvents {
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Event>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM events WHERE day BETWEEN ? AND ? ORDER BY day, start_min").map_err(store)?;
        let rows = stmt.query_map(params![first.to_string(), last.to_string()], |r| Ok(Event {
            day: day(r.get("day")?).unwrap_or(first),
            time: TimeRange { start: r.get("start_min")?, end: r.get("end_min")? },
            title: r.get("title")?, details: r.get("details")?, color: r.get("color")?, id: Some(r.get("id")?),
        })).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
}

pub struct SqliteRoutines(pub Rc<Database>);

impl RoutineRepository for SqliteRoutines {
    fn list(&self) -> AppResult<Vec<Routine>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM routines ORDER BY start_min, title COLLATE NOCASE").map_err(store)?;
        let rows = stmt.query_map([], |r| {
            let days: String = r.get("weekdays")?;
            let mut weekdays: Vec<u8> = days.split(',').filter_map(|d| d.trim().parse().ok()).collect();
            weekdays.sort();
            weekdays.dedup();
            Ok(Routine { title: r.get("title")?, minute: r.get("start_min")?, weekdays, color: r.get("color")?, id: Some(r.get("id")?) })
        }).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
}

pub struct SqliteJournal(pub Rc<Database>);

impl JournalRepository for SqliteJournal {
    fn get(&self, d: NaiveDate) -> AppResult<String> {
        Ok(self.0.conn.query_row("SELECT body FROM journal WHERE day = ?", [d.to_string()], |r| r.get(0))
            .optional().map_err(store)?.unwrap_or_default())
    }

    fn put(&self, d: NaiveDate, body: &str) -> AppResult {
        self.0.conn.execute("INSERT INTO journal (day, body) VALUES (?, ?) ON CONFLICT(day) DO UPDATE SET body = excluded.body",
            params![d.to_string(), body]).map(|_| ()).map_err(store)
    }
}

pub struct SqliteJobs(pub Rc<Database>);

impl JobRepository for SqliteJobs {
    fn list(&self) -> AppResult<Vec<Job>> {
        let conn = &self.0.conn;
        let mut stmt = conn.prepare("SELECT * FROM jobs ORDER BY name COLLATE NOCASE").map_err(store)?;
        let mut jobs: Vec<Job> = stmt.query_map([], |r| {
            let mut j = Job::named(&r.get::<_, String>("name")?);
            j.color = r.get("color")?;
            j.hourly_rate = r.get("hourly_rate")?;
            j.id = Some(r.get("id")?);
            j.deductions = r.get("deductions")?;
            j.pay_mode = r.get("pay_mode")?;
            j.monthly_pay = r.get("monthly_pay")?;
            j.mensilities = r.get("mensilities")?;
            j.contract_start = day(r.get("contract_start")?);
            j.contract_end = day(r.get("contract_end")?);
            j.tax_model = r.get("tax_model")?;
            j.inps = r.get("inps")?;
            j.addizionali = r.get("addizionali")?;
            j.fixed_term = r.get::<_, i64>("fixed_term")? != 0;
            j.cuneo = r.get::<_, i64>("cuneo")? != 0;
            Ok(j)
        }).map_err(store)?.collect::<Result<_, _>>().map_err(store)?;
        let mut pats = conn.prepare("SELECT * FROM shift_patterns ORDER BY weekday, start_min").map_err(store)?;
        let rows = pats.query_map([], |p| Ok((p.get::<_, i64>("job_id")?, ShiftPattern {
            weekday: p.get("weekday")?, start: p.get("start_min")?, duration: p.get("duration_min")?,
            break_minutes: p.get("break_min")?, since: day(p.get("since")?), until: day(p.get("until")?), id: Some(p.get("id")?),
        }))).map_err(store)?;
        for row in rows {
            let (job, pattern) = row.map_err(store)?;
            if let Some(j) = jobs.iter_mut().find(|j| j.id == Some(job)) {
                j.schedule.push(pattern);
            }
        }
        Ok(jobs)
    }
}

pub struct SqliteShifts(pub Rc<Database>);

impl ShiftRepository for SqliteShifts {
    fn starting_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Shift>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM shifts WHERE day BETWEEN ? AND ? ORDER BY day, start_min").map_err(store)?;
        let rows = stmt.query_map(params![first.to_string(), last.to_string()], |r| Ok(Shift {
            job_id: r.get("job_id")?, day: day(r.get("day")?).unwrap_or(first), start: r.get("start_min")?,
            duration: r.get("duration_min")?, break_minutes: r.get("break_min")?, notes: r.get("notes")?,
            id: Some(r.get("id")?), recurring: false,
        })).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    fn skipped(&self, first: NaiveDate, last: NaiveDate) -> AppResult<HashSet<(i64, NaiveDate, i32)>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM shift_skips WHERE day BETWEEN ? AND ?").map_err(store)?;
        let rows = stmt.query_map(params![first.to_string(), last.to_string()], |r| Ok((
            r.get::<_, i64>("job_id")?, day(r.get("day")?).unwrap_or(first), r.get::<_, i32>("start_min")?))).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
}
