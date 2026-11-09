//! The SQLite database: the Python app's schema (version 18) and migrations, unchanged, so either app can open the
//! same file (port of `quire/infrastructure/sqlite.py`).

use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub const SCHEMA_VERSION: i32 = 19;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS courses (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL,
    teacher  TEXT NOT NULL DEFAULT '',
    room     TEXT NOT NULL DEFAULT '',
    color    TEXT NOT NULL DEFAULT '#4f7cff',
    external_id TEXT
);
CREATE TABLE IF NOT EXISTS class_slots (
    id        INTEGER PRIMARY KEY,
    course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    weekday   INTEGER NOT NULL,
    start_min INTEGER NOT NULL,
    end_min   INTEGER NOT NULL,
    room      TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS events (
    id        INTEGER PRIMARY KEY,
    day       TEXT NOT NULL,
    start_min INTEGER NOT NULL,
    end_min   INTEGER NOT NULL,
    title     TEXT NOT NULL,
    details   TEXT NOT NULL DEFAULT '',
    color     TEXT NOT NULL DEFAULT '#8a8f98'
);
CREATE TABLE IF NOT EXISTS routines (
    id        INTEGER PRIMARY KEY,
    title     TEXT NOT NULL,
    start_min INTEGER NOT NULL,
    weekdays  TEXT NOT NULL DEFAULT '0,1,2,3,4,5,6',
    color     TEXT NOT NULL DEFAULT '#f5a524'
);
CREATE TABLE IF NOT EXISTS tasks (
    id        INTEGER PRIMARY KEY,
    title     TEXT NOT NULL,
    kind      TEXT NOT NULL DEFAULT 'task',
    course_id INTEGER REFERENCES courses(id) ON DELETE SET NULL,
    due       TEXT,
    done      INTEGER NOT NULL DEFAULT 0,
    details   TEXT NOT NULL DEFAULT '',
    external_id TEXT
);
CREATE TABLE IF NOT EXISTS notebooks (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#8a8f98'
);
CREATE TABLE IF NOT EXISTS notes (
    id        INTEGER PRIMARY KEY,
    title     TEXT NOT NULL DEFAULT 'Untitled',
    body      TEXT NOT NULL DEFAULT '',
    course_id INTEGER REFERENCES courses(id) ON DELETE SET NULL,
    pinned    INTEGER NOT NULL DEFAULT 0,
    updated   TEXT NOT NULL,
    topic     TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS note_images (
    id    INTEGER PRIMARY KEY,
    uid   TEXT,
    mime  TEXT NOT NULL,
    data  BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS journal (
    day  TEXT PRIMARY KEY,
    body TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS sleep_logs (
    day     TEXT PRIMARY KEY,
    asleep  INTEGER NOT NULL,
    woke    INTEGER NOT NULL,
    quality INTEGER,
    note    TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS water_logs (
    id     INTEGER PRIMARY KEY,
    day    TEXT NOT NULL,
    at_min INTEGER NOT NULL,
    ml     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_water_day ON water_logs(day);
CREATE TABLE IF NOT EXISTS medicines (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL,
    dose     TEXT NOT NULL DEFAULT '',
    times    TEXT NOT NULL DEFAULT '',
    weekdays TEXT NOT NULL DEFAULT '',
    active   INTEGER NOT NULL DEFAULT 1,
    notes    TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS medicine_doses (
    id          INTEGER PRIMARY KEY,
    medicine_id INTEGER NOT NULL REFERENCES medicines(id) ON DELETE CASCADE,
    day         TEXT NOT NULL,
    planned     INTEGER NOT NULL,
    taken       INTEGER NOT NULL DEFAULT 1,
    at_min      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_dose_day ON medicine_doses(day);
CREATE TABLE IF NOT EXISTS transactions (
    id       INTEGER PRIMARY KEY,
    day      TEXT NOT NULL,
    cents    INTEGER NOT NULL,
    income   INTEGER NOT NULL DEFAULT 0,
    category TEXT NOT NULL DEFAULT '',
    note     TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_transaction_day ON transactions(day);
CREATE TABLE IF NOT EXISTS grades (
    external_id TEXT PRIMARY KEY,
    subject     TEXT NOT NULL,
    day         TEXT NOT NULL,
    display     TEXT NOT NULL,
    value       REAL,
    component   TEXT NOT NULL DEFAULT '',
    period      TEXT NOT NULL DEFAULT '',
    notes       TEXT NOT NULL DEFAULT '',
    cancelled   INTEGER NOT NULL DEFAULT 0,
    course_id   INTEGER REFERENCES courses(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS lessons (
    external_id TEXT PRIMARY KEY,
    day         TEXT NOT NULL,
    subject     TEXT NOT NULL,
    topic       TEXT NOT NULL DEFAULT '',
    teacher     TEXT NOT NULL DEFAULT '',
    hour        INTEGER NOT NULL DEFAULT 0,
    course_id   INTEGER REFERENCES courses(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS register_subjects (
    external_id TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    teachers    TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS jobs (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '#0090ff',
    hourly_rate REAL,
    deductions  REAL NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS shifts (
    id            INTEGER PRIMARY KEY,
    job_id        INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    day           TEXT NOT NULL,
    start_min     INTEGER NOT NULL,
    duration_min  INTEGER NOT NULL,
    break_min     INTEGER NOT NULL DEFAULT 0,
    notes         TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_shifts_day ON shifts(day);
CREATE TABLE IF NOT EXISTS shift_patterns (
    id           INTEGER PRIMARY KEY,
    job_id       INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    weekday      INTEGER NOT NULL,
    start_min    INTEGER NOT NULL,
    duration_min INTEGER NOT NULL,
    break_min    INTEGER NOT NULL DEFAULT 0,
    since        TEXT,
    until        TEXT
);
CREATE TABLE IF NOT EXISTS shift_skips (
    job_id    INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    day       TEXT NOT NULL,
    start_min INTEGER NOT NULL,
    PRIMARY KEY (job_id, day, start_min)
);
CREATE TABLE IF NOT EXISTS focus_sessions (
    id       INTEGER PRIMARY KEY,
    started  TEXT NOT NULL,
    minutes  INTEGER NOT NULL,
    task_id  INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
    label    TEXT NOT NULL DEFAULT '',
    complete INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_focus_started ON focus_sessions(started);
CREATE TABLE IF NOT EXISTS register_items (
    kind        TEXT NOT NULL,      -- 'absence', 'notice', 'book', 'document'
    external_id TEXT NOT NULL,
    day         TEXT,
    data        TEXT NOT NULL,      -- JSON
    PRIMARY KEY (kind, external_id)
);
CREATE TABLE IF NOT EXISTS school_days (day TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_day ON events(day);
CREATE INDEX IF NOT EXISTS idx_lessons_day ON lessons(day);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due);
"#;

/// Columns added by later versions: (table, column, definition), in the Python app's order.
const ADDED_COLUMNS: &[(&str, &str, &str)] = &[
    // v2: sync ids for records imported from a school register.
    ("courses", "external_id", "TEXT"),
    ("tasks", "external_id", "TEXT"),
    // v6: share of pay withheld for tax per job.
    ("jobs", "deductions", "REAL NOT NULL DEFAULT 0"),
    // v8: what a focus session was spent on (task title or ad-hoc project).
    ("focus_sessions", "label", "TEXT NOT NULL DEFAULT ''"),
    // v11: focus sessions cut short (skipped or reset) are logged as not complete.
    ("focus_sessions", "complete", "INTEGER NOT NULL DEFAULT 1"),
    // v3: topics group notes within a course.
    ("notes", "topic", "TEXT NOT NULL DEFAULT ''"),
    // v13: notebooks, groups of notes that aren't classes.
    ("notes", "notebook_id", "INTEGER REFERENCES notebooks(id) ON DELETE SET NULL"),
    // v14: monthly pay, contract dates and the Italian tax model per job.
    ("jobs", "pay_mode", "TEXT NOT NULL DEFAULT 'hourly'"),
    ("jobs", "monthly_pay", "REAL"),
    ("jobs", "mensilities", "INTEGER NOT NULL DEFAULT 13"),
    ("jobs", "contract_start", "TEXT"),
    ("jobs", "contract_end", "TEXT"),
    ("jobs", "tax_model", "TEXT NOT NULL DEFAULT 'flat'"),
    ("jobs", "inps", "REAL NOT NULL DEFAULT 9.19"),
    ("jobs", "addizionali", "REAL NOT NULL DEFAULT 0"),
    ("jobs", "fixed_term", "INTEGER NOT NULL DEFAULT 1"),
    ("jobs", "cuneo", "INTEGER NOT NULL DEFAULT 0"),
    // v19: payday (the pay added to Money by itself), as in the Python app.
    ("jobs", "payday", "INTEGER"),
    ("jobs", "pays_previous_month", "INTEGER NOT NULL DEFAULT 1"),
    ("jobs", "payday_since", "TEXT"),
];

/// Owns the connection.
pub struct Database {
    pub path: PathBuf,
    pub conn: Connection,
}

impl Database {
    /// Opens (or creates) the database and brings it up to `SCHEMA_VERSION`.
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)?;
        let db = Self { path, conn };
        db.conn.execute_batch("PRAGMA foreign_keys = ON")?;
        db.migrate()?;
        Ok(db)
    }

    /// A database that lives only in memory (tests, the demo).
    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::open(":memory:")
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        for (table, column, definition) in ADDED_COLUMNS {
            self.add_column(table, column, definition)?;
        }
        self.conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_external ON tasks(external_id) WHERE external_id IS NOT NULL",
        )?;
        // v9: change tracking for sync between devices (ids, timestamps, tombstones).
        super::sync_store::install(&self.conn)?;
        self.conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        Ok(())
    }

    fn add_column(&self, table: &str, column: &str, definition: &str) -> rusqlite::Result<()> {
        if !columns(&self.conn, table)?.iter().any(|c| c == column) {
            self.conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"))?;
        }
        Ok(())
    }

    pub fn backup_to(&self, dest: impl AsRef<Path>) -> rusqlite::Result<()> {
        let mut out = Connection::open(dest)?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut out)?;
        backup.run_to_completion(256, std::time::Duration::ZERO, None)
    }
}

pub fn columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?.collect();
    names
}
