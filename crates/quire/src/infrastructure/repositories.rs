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

    fn add(&self, c: &Course) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO courses (name, teacher, room, color, external_id) VALUES (?, ?, ?, ?, ?)",
            params![c.name, c.teacher, c.room, c.color, c.external_id]).map_err(store)?;
        let id = self.0.conn.last_insert_rowid();
        self.write_slots(id, c)?;
        Ok(id)
    }

    fn update(&self, c: &Course) -> AppResult {
        self.0.conn.execute("UPDATE courses SET name = ?, teacher = ?, room = ?, color = ?, external_id = ? WHERE id = ?",
            params![c.name, c.teacher, c.room, c.color, c.external_id, c.id]).map_err(store)?;
        self.write_slots(c.id.unwrap_or_default(), c)
    }

    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM courses WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}

impl SqliteCourses {
    fn write_slots(&self, id: i64, c: &Course) -> AppResult {
        self.0.conn.execute("DELETE FROM class_slots WHERE course_id = ?", [id]).map_err(store)?;
        for s in &c.slots {
            self.0.conn.execute("INSERT INTO class_slots (course_id, weekday, start_min, end_min, room) VALUES (?, ?, ?, ?, ?)",
                params![id, s.weekday, s.time.start, s.time.end, s.room]).map_err(store)?;
        }
        Ok(())
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

fn event(r: &Row) -> rusqlite::Result<Event> {
    Ok(Event {
        day: day(r.get("day")?).unwrap_or_default(),
        time: TimeRange { start: r.get("start_min")?, end: r.get("end_min")? },
        title: r.get("title")?, details: r.get("details")?, color: r.get("color")?, id: Some(r.get("id")?),
    })
}

impl EventRepository for SqliteEvents {
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<Event>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM events WHERE day BETWEEN ? AND ? ORDER BY day, start_min").map_err(store)?;
        let rows = stmt.query_map(params![first.to_string(), last.to_string()], event).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    fn get(&self, id: i64) -> AppResult<Option<Event>> {
        self.0.conn.query_row("SELECT * FROM events WHERE id = ?", [id], event).optional().map_err(store)
    }

    fn add(&self, e: &Event) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO events (day, start_min, end_min, title, details, color) VALUES (?, ?, ?, ?, ?, ?)",
            params![e.day.to_string(), e.time.start, e.time.end, e.title, e.details, e.color]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }

    fn update(&self, e: &Event) -> AppResult {
        self.0.conn.execute("UPDATE events SET day = ?, start_min = ?, end_min = ?, title = ?, details = ?, color = ? WHERE id = ?",
            params![e.day.to_string(), e.time.start, e.time.end, e.title, e.details, e.color, e.id]).map(|_| ()).map_err(store)
    }

    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM events WHERE id = ?", [id]).map(|_| ()).map_err(store)
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

    fn add(&self, r: &Routine) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO routines (title, start_min, weekdays, color) VALUES (?, ?, ?, ?)",
            params![r.title, r.minute, weekdays(&r.weekdays), r.color]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }

    fn update(&self, r: &Routine) -> AppResult {
        self.0.conn.execute("UPDATE routines SET title = ?, start_min = ?, weekdays = ?, color = ? WHERE id = ?",
            params![r.title, r.minute, weekdays(&r.weekdays), r.color, r.id]).map(|_| ()).map_err(store)
    }

    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM routines WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}

fn weekdays(days: &[u8]) -> String {
    days.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",")
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

// ---- notes ---------------------------------------------------------------------------------------------------------

use crate::application::ports::{CourseLookup, ImageRepository, NoteRepository, NotebookRepository};
use crate::domain::model::{Note, NoteImage, Notebook};

impl CourseLookup for SqliteCourses {
    fn get(&self, id: i64) -> AppResult<Option<Course>> {
        Ok(self.list()?.into_iter().find(|c| c.id == Some(id)))
    }
}

pub struct SqliteNotes(pub Rc<Database>);

fn note(r: &Row) -> rusqlite::Result<Note> {
    let updated: Option<String> = r.get("updated")?;
    Ok(Note {
        body: r.get("body")?, course_id: r.get("course_id")?, pinned: r.get::<_, i64>("pinned")? != 0,
        updated: updated.and_then(|u| NaiveDateTime::parse_from_str(&u, "%Y-%m-%dT%H:%M:%S").ok()),
        id: Some(r.get("id")?), topic: r.get("topic")?, notebook_id: r.get("notebook_id")?,
    })
}

fn updated(n: &Note) -> String {
    stamp(&n.updated.unwrap_or_else(|| chrono::Local::now().naive_local()))
}

fn like(text: &str) -> String {
    format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))
}

impl NoteRepository for SqliteNotes {
    fn search(&self, text: &str, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<Note>> {
        let mut wh: Vec<String> = Vec::new();
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if !text.is_empty() {
            wh.push("(body LIKE ? ESCAPE '\\' OR topic LIKE ? ESCAPE '\\')".into());
            args.push(like(text).into());
            args.push(like(text).into());
        }
        if let Some(c) = course_id {
            wh.push("course_id = ?".into());
            args.push(c.into());
        }
        if let Some(b) = notebook_id {
            wh.push("notebook_id = ?".into());
            args.push(b.into());
        }
        let sql = format!("SELECT * FROM notes{} ORDER BY pinned DESC, updated DESC, id DESC",
                          if wh.is_empty() { String::new() } else { format!(" WHERE {}", wh.join(" AND ")) });
        let mut stmt = self.0.conn.prepare(&sql).map_err(store)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), note).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    fn find_by_title(&self, title: &str, course_id: Option<i64>) -> AppResult<Option<Note>> {
        self.0.conn.query_row("SELECT * FROM notes WHERE title = ? AND course_id IS ? ORDER BY id LIMIT 1",
            params![title, course_id], note).optional().map_err(store)
    }

    fn get(&self, id: i64) -> AppResult<Option<Note>> {
        self.0.conn.query_row("SELECT * FROM notes WHERE id = ?", [id], note).optional().map_err(store)
    }

    fn topics(&self, course_id: Option<i64>, notebook_id: Option<i64>) -> AppResult<Vec<String>> {
        let mut stmt = self.0.conn.prepare(
            "SELECT DISTINCT topic FROM notes WHERE course_id IS ? AND notebook_id IS ? AND topic != ''").map_err(store)?;
        let rows = stmt.query_map(params![course_id, notebook_id], |r| r.get(0)).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    fn rename_topic(&self, course_id: Option<i64>, old: &str, new: &str, notebook_id: Option<i64>) -> AppResult<usize> {
        // leaves `updated` alone: refiling isn't editing
        self.0.conn.execute("UPDATE notes SET topic = ? WHERE course_id IS ? AND notebook_id IS ? AND topic = ?",
            params![new, course_id, notebook_id, old]).map_err(store)
    }

    fn add(&self, n: &Note) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO notes (title, body, course_id, pinned, updated, topic, notebook_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![n.title(), n.body, n.course_id, n.pinned as i64, updated(n), n.topic, n.notebook_id]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }

    fn update(&self, n: &Note) -> AppResult {
        self.0.conn.execute("UPDATE notes SET title = ?, body = ?, course_id = ?, pinned = ?, updated = ?, topic = ?, notebook_id = ? WHERE id = ?",
            params![n.title(), n.body, n.course_id, n.pinned as i64, updated(n), n.topic, n.notebook_id, n.id]).map(|_| ()).map_err(store)
    }

    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM notes WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}

pub struct SqliteNotebooks(pub Rc<Database>);

fn notebook(r: &Row) -> rusqlite::Result<Notebook> {
    Ok(Notebook { name: r.get("name")?, color: r.get("color")?, id: Some(r.get("id")?) })
}

impl NotebookRepository for SqliteNotebooks {
    fn list(&self) -> AppResult<Vec<Notebook>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM notebooks ORDER BY name").map_err(store)?;
        let rows = stmt.query_map([], notebook).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
    fn get(&self, id: i64) -> AppResult<Option<Notebook>> {
        self.0.conn.query_row("SELECT * FROM notebooks WHERE id = ?", [id], notebook).optional().map_err(store)
    }
    fn add(&self, b: &Notebook) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO notebooks (name, color) VALUES (?, ?)", params![b.name, b.color]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }
    fn update(&self, b: &Notebook) -> AppResult {
        self.0.conn.execute("UPDATE notebooks SET name = ?, color = ? WHERE id = ?", params![b.name, b.color, b.id]).map(|_| ()).map_err(store)
    }
    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM notebooks WHERE id = ?", [id]).map(|_| ()).map_err(store) // its notes stay, unfiled
    }
    fn note_count(&self, id: i64) -> AppResult<usize> {
        self.0.conn.query_row("SELECT COUNT(*) FROM notes WHERE notebook_id = ?", [id], |r| r.get::<_, i64>(0))
            .map(|n| n as usize).map_err(store)
    }
}

pub struct SqliteImages(pub Rc<Database>);

impl ImageRepository for SqliteImages {
    fn get(&self, uid: &str) -> AppResult<Option<NoteImage>> {
        self.0.conn.query_row("SELECT * FROM note_images WHERE uid = ?", [uid], |r| Ok(NoteImage {
            uid: r.get("uid")?, mime: r.get("mime")?, data: r.get("data")?, id: Some(r.get("id")?),
        })).optional().map_err(store)
    }
    fn add(&self, i: &NoteImage) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO note_images (uid, mime, data) VALUES (?, ?, ?)", params![i.uid, i.mime, i.data]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }
    fn replace(&self, uid: &str, mime: &str, data: &[u8]) -> AppResult {
        self.0.conn.execute("UPDATE note_images SET mime = ?, data = ? WHERE uid = ?", params![mime, data, uid]).map(|_| ()).map_err(store)
    }
    fn new_uid(&self) -> String {
        self.0.conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0)).unwrap_or_default()
    }
}

// ---- water ---------------------------------------------------------------------------------------------------------

use crate::application::ports::WaterRepository;
use crate::domain::water::WaterLog;

pub struct SqliteWater(pub Rc<Database>);

impl WaterRepository for SqliteWater {
    fn between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<WaterLog>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM water_logs WHERE day BETWEEN ? AND ? ORDER BY day, at_min, id").map_err(store)?;
        let rows = stmt.query_map(params![first.to_string(), last.to_string()], |r| Ok(WaterLog {
            day: day(r.get("day")?).unwrap_or(first), at: r.get("at_min")?, ml: r.get("ml")?, id: Some(r.get("id")?),
        })).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
    fn add(&self, l: &WaterLog) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO water_logs (day, at_min, ml) VALUES (?, ?, ?)", params![l.day.to_string(), l.at, l.ml]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }
    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM water_logs WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}

// ---- medicines -----------------------------------------------------------------------------------------------------

use crate::application::ports::MedicineRepository;
use crate::domain::medicine::{DoseLog, Medicine};

fn numbers<T: std::str::FromStr>(text: &str) -> Vec<T> {
    text.split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

fn joined<T: ToString>(items: &[T]) -> String {
    items.iter().map(T::to_string).collect::<Vec<_>>().join(",")
}

fn medicine(r: &Row) -> rusqlite::Result<Medicine> {
    Ok(Medicine {
        name: r.get("name")?, dose: r.get("dose")?, times: numbers(&r.get::<_, String>("times")?),
        weekdays: numbers(&r.get::<_, String>("weekdays")?), active: r.get::<_, i64>("active")? != 0,
        notes: r.get("notes")?, id: Some(r.get("id")?),
    })
}

pub struct SqliteMedicines(pub Rc<Database>);

impl MedicineRepository for SqliteMedicines {
    fn all(&self) -> AppResult<Vec<Medicine>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM medicines ORDER BY name COLLATE NOCASE, id").map_err(store)?;
        let rows = stmt.query_map([], medicine).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
    fn get(&self, id: i64) -> AppResult<Option<Medicine>> {
        self.0.conn.query_row("SELECT * FROM medicines WHERE id = ?", [id], medicine).optional().map_err(store)
    }
    fn add(&self, m: &Medicine) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO medicines (name, dose, times, weekdays, active, notes) VALUES (?, ?, ?, ?, ?, ?)",
            params![m.name, m.dose, joined(&m.times), joined(&m.weekdays), m.active as i64, m.notes]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }
    fn update(&self, m: &Medicine) -> AppResult {
        self.0.conn.execute("UPDATE medicines SET name = ?, dose = ?, times = ?, weekdays = ?, active = ?, notes = ? WHERE id = ?",
            params![m.name, m.dose, joined(&m.times), joined(&m.weekdays), m.active as i64, m.notes, m.id]).map(|_| ()).map_err(store)
    }
    fn delete(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM medicines WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
    fn logs_between(&self, first: NaiveDate, last: NaiveDate) -> AppResult<Vec<DoseLog>> {
        let mut stmt = self.0.conn.prepare("SELECT * FROM medicine_doses WHERE day BETWEEN ? AND ? ORDER BY day, id").map_err(store)?;
        let rows = stmt.query_map(params![first.to_string(), last.to_string()], |r| Ok(DoseLog {
            medicine_id: r.get("medicine_id")?, day: day(r.get("day")?).unwrap_or(first), planned: r.get("planned")?,
            taken: r.get::<_, i64>("taken")? != 0, at: r.get("at_min")?, id: Some(r.get("id")?),
        })).map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
    fn add_log(&self, l: &DoseLog) -> AppResult<i64> {
        self.0.conn.execute("INSERT INTO medicine_doses (medicine_id, day, planned, taken, at_min) VALUES (?, ?, ?, ?, ?)",
            params![l.medicine_id, l.day.to_string(), l.planned, l.taken as i64, l.at]).map_err(store)?;
        Ok(self.0.conn.last_insert_rowid())
    }
    fn delete_log(&self, id: i64) -> AppResult {
        self.0.conn.execute("DELETE FROM medicine_doses WHERE id = ?", [id]).map(|_| ()).map_err(store)
    }
}
