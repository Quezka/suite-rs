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
