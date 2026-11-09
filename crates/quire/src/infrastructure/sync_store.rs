//! This device's side of sync, in SQLite (port of `quire/infrastructure/sync_store.py`; records must stay
//! byte-compatible with it and with the phone's `Codec.kt`).
//!
//! Change tracking is done by triggers, so the repositories don't know sync exists: every synced table has `uid`
//! (the same on every device), `modified` (UTC, ms) and `dirty`; inserting, updating or deleting a row (or one of its
//! child rows, like a course's class times) marks it dirty and stamps `modified`; deletions leave a tombstone. While
//! remote changes are being applied, a row in `sync_flags` switches the triggers off.

use std::collections::{HashMap, HashSet};

use base64::Engine;
use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Map, Value};

use crate::application::ports::SyncRecord;

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";
const RANDOM_UID: &str = "lower(hex(randomblob(16)))";
const NOT_APPLYING: &str = "NOT EXISTS (SELECT 1 FROM sync_flags WHERE name = 'applying')";
const EXTERNAL_UID: &str = "CASE WHEN NEW.external_id IS NOT NULL THEN 'ext:' || NEW.external_id ELSE lower(hex(randomblob(16))) END";

pub struct Synced {
    pub kind: &'static str,
    pub table: &'static str,
    /// SQL for a new row's uid, in terms of NEW.<column>.
    pub uid: &'static str,
    /// (child table, column pointing at the parent).
    pub children: &'static [(&'static str, &'static str)],
}

/// Parents before children, so references resolve when a batch is applied in this order. Things imported from the
/// school register get the same uid on every device, so two devices importing the same homework don't make copies.
pub const SYNCED: &[Synced] = &[
    Synced { kind: "course", table: "courses", uid: EXTERNAL_UID, children: &[("class_slots", "course_id")] },
    Synced { kind: "job", table: "jobs", uid: RANDOM_UID, children: &[("shift_patterns", "job_id"), ("shift_skips", "job_id")] },
    Synced { kind: "event", table: "events", uid: RANDOM_UID, children: &[] },
    Synced { kind: "routine", table: "routines", uid: RANDOM_UID, children: &[] },
    Synced { kind: "task", table: "tasks", uid: EXTERNAL_UID, children: &[] },
    // Pictures in notes and groups of notes, before the notes.
    Synced { kind: "image", table: "note_images", uid: RANDOM_UID, children: &[] },
    Synced { kind: "notebook", table: "notebooks", uid: RANDOM_UID, children: &[] },
    Synced { kind: "note", table: "notes", uid: RANDOM_UID, children: &[] },
    Synced { kind: "journal", table: "journal", uid: "'day:' || NEW.day", children: &[] },
    // One record per glass.
    Synced { kind: "water", table: "water_logs", uid: RANDOM_UID, children: &[] },
    // One night per day, like the journal.
    Synced { kind: "sleep", table: "sleep_logs", uid: "'day:' || NEW.day", children: &[] },
    Synced { kind: "shift", table: "shifts", uid: RANDOM_UID, children: &[] },
    Synced { kind: "focus", table: "focus_sessions", uid: RANDOM_UID, children: &[] },
    Synced { kind: "medicine", table: "medicines", uid: RANDOM_UID, children: &[] },
    // After its medicine.
    Synced { kind: "dose", table: "medicine_doses", uid: RANDOM_UID, children: &[] },
    Synced { kind: "money", table: "transactions", uid: RANDOM_UID, children: &[] },
];

pub fn by_kind(kind: &str) -> Option<(usize, &'static Synced)> {
    SYNCED.iter().enumerate().find(|(_, s)| s.kind == kind)
}

/// Schema v9: sync columns, tombstones and triggers (idempotent).
pub fn install(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sync_flags (name TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS sync_tombstones (
            kind TEXT NOT NULL, uid TEXT NOT NULL, modified TEXT NOT NULL,
            dirty INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (kind, uid));
        CREATE TABLE IF NOT EXISTS sync_inbox (
            kind TEXT NOT NULL, uid TEXT NOT NULL, record TEXT NOT NULL,
            PRIMARY KEY (kind, uid));
    ",
    )?;
    // A crash mid-apply mustn't leave tracking off.
    conn.execute_batch("DELETE FROM sync_flags")?;
    for s in SYNCED {
        let existing = super::sqlite::columns(conn, s.table)?;
        for (column, definition) in [("uid", "TEXT"), ("modified", "TEXT"), ("dirty", "INTEGER NOT NULL DEFAULT 0")] {
            if !existing.iter().any(|c| c == column) {
                conn.execute_batch(&format!("ALTER TABLE {} ADD COLUMN {column} {definition}", s.table))?;
            }
        }
        // Rows from before sync existed: give them ids and send them the first time.
        let backfill = s.uid.replace("NEW.", "");
        conn.execute_batch(&format!("UPDATE {} SET uid = {backfill}, modified = {NOW}, dirty = 1 WHERE uid IS NULL", s.table))?;
        conn.execute_batch(&format!("CREATE UNIQUE INDEX IF NOT EXISTS idx_{t}_uid ON {t}(uid)", t = s.table))?;
        conn.execute_batch(&format!(
            "
            CREATE TRIGGER IF NOT EXISTS sync_{t}_ins AFTER INSERT ON {t}
            WHEN {NOT_APPLYING} BEGIN
                UPDATE {t} SET uid = COALESCE(NEW.uid, {u}), modified = {NOW},
                    dirty = 1 WHERE rowid = NEW.rowid;
                DELETE FROM sync_tombstones WHERE kind = '{k}'
                    AND uid = (SELECT uid FROM {t} WHERE rowid = NEW.rowid);
            END;
            CREATE TRIGGER IF NOT EXISTS sync_{t}_upd AFTER UPDATE ON {t}
            WHEN {NOT_APPLYING} BEGIN
                UPDATE {t} SET modified = {NOW}, dirty = 1 WHERE rowid = NEW.rowid;
            END;
            CREATE TRIGGER IF NOT EXISTS sync_{t}_del AFTER DELETE ON {t}
            WHEN {NOT_APPLYING} AND OLD.uid IS NOT NULL BEGIN
                INSERT OR REPLACE INTO sync_tombstones (kind, uid, modified, dirty)
                VALUES ('{k}', OLD.uid, {NOW}, 1);
            END;
        ",
            t = s.table,
            u = s.uid,
            k = s.kind
        ))?;
        for (child, fk) in s.children {
            for (event, row) in [("INSERT", "NEW"), ("UPDATE", "NEW"), ("DELETE", "OLD")] {
                conn.execute_batch(&format!(
                    "
                    CREATE TRIGGER IF NOT EXISTS sync_{child}_{e}
                    AFTER {event} ON {child} WHEN {NOT_APPLYING} BEGIN
                        UPDATE {t} SET modified = {NOW}, dirty = 1
                        WHERE id = {row}.{fk};
                    END",
                    e = event.to_lowercase(),
                    t = s.table
                ))?;
            }
        }
    }
    Ok(())
}

pub fn dump(record: &SyncRecord) -> String {
    json!({"kind": record.kind, "uid": record.uid, "modified": record.modified, "deleted": record.deleted, "data": record.data}).to_string()
}

pub fn load(text: &str) -> Option<SyncRecord> {
    let raw: Value = serde_json::from_str(text).ok()?;
    Some(SyncRecord {
        kind: raw["kind"].as_str()?.into(),
        uid: raw["uid"].as_str()?.into(),
        modified: raw["modified"].as_str()?.into(),
        deleted: raw["deleted"].as_bool().unwrap_or(false),
        data: raw.get("data").filter(|d| !d.is_null()).cloned(),
    })
}

/// A column as JSON (null for NULL; numbers as numbers; text as text).
fn cell(row: &Row, name: &str) -> Value {
    match row.get_ref(name) {
        Ok(ValueRef::Null) | Err(_) => Value::Null,
        Ok(ValueRef::Integer(i)) => json!(i),
        Ok(ValueRef::Real(f)) => json!(f),
        Ok(ValueRef::Text(t)) => json!(String::from_utf8_lossy(t)),
        Ok(ValueRef::Blob(b)) => json!(base64::engine::general_purpose::STANDARD.encode(b)),
    }
}

fn flag(row: &Row, name: &str) -> Value {
    json!(row.get::<_, i64>(name).unwrap_or(0) != 0)
}

/// "1,2,5" → [1, 2, 5], skipping anything that isn't a number (as the Python app does).
fn number_list(text: &str) -> Value {
    json!(text.split(',').filter_map(|p| { let p = p.trim(); (!p.is_empty() && p.chars().all(|c| c.is_ascii_digit())).then(|| p.parse::<i64>().ok()).flatten() }).collect::<Vec<_>>())
}

/// A JSON value as a SQL value for writing.
fn sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n.as_i64().map_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0)), Sql::Integer),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

/// Python's `bool(x)` for a JSON value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn joined(v: Option<&Value>, default: &[i64]) -> String {
    match v.and_then(Value::as_array) {
        Some(items) => items.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","),
        None => default.iter().map(i64::to_string).collect::<Vec<_>>().join(","),
    }
}

pub struct SyncStore<'a> {
    conn: &'a Connection,
}

impl<'a> SyncStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// One transaction with change tracking off (so applied changes aren't echoed).
    fn applying<T>(&self, work: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
        self.conn.execute_batch("BEGIN; INSERT OR IGNORE INTO sync_flags VALUES ('applying')")?;
        let result = work(self.conn);
        let end = if result.is_ok() { "DELETE FROM sync_flags WHERE name = 'applying'; COMMIT" } else { "ROLLBACK" };
        self.conn.execute_batch(end)?;
        result
    }

    fn uid(&self, table: &str, row_id: Option<i64>) -> Value {
        let Some(id) = row_id else { return Value::Null };
        self.conn
            .query_row(&format!("SELECT uid FROM {table} WHERE id = ?"), [id], |r| r.get::<_, Option<String>>(0))
            .optional()
            .ok()
            .flatten()
            .flatten()
            .map_or(Value::Null, Value::String)
    }

    fn id(&self, table: &str, uid: Option<&Value>) -> Value {
        let Some(uid) = uid.and_then(Value::as_str) else { return Value::Null };
        self.conn
            .query_row(&format!("SELECT id FROM {table} WHERE uid = ?"), [uid], |r| r.get::<_, i64>(0))
            .optional()
            .ok()
            .flatten()
            .map_or(Value::Null, |i| json!(i))
    }

    // ---- outgoing ---------------------------------------------------------------------------------------------------

    pub fn outgoing(&self) -> rusqlite::Result<Vec<SyncRecord>> {
        let mut records = Vec::new();
        for s in SYNCED {
            let mut stmt = self.conn.prepare(&format!("SELECT * FROM {} WHERE dirty = 1", s.table))?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                records.push(SyncRecord {
                    kind: s.kind.into(),
                    uid: row.get("uid")?,
                    modified: row.get("modified")?,
                    deleted: false,
                    data: Some(self.export(s.kind, row)?),
                });
            }
        }
        let mut stmt = self.conn.prepare("SELECT kind, uid, modified FROM sync_tombstones WHERE dirty = 1")?;
        let tombstones = stmt.query_map([], |r| {
            Ok(SyncRecord { kind: r.get(0)?, uid: r.get(1)?, modified: r.get(2)?, deleted: true, data: None })
        })?;
        for t in tombstones {
            records.push(t?);
        }
        Ok(records)
    }

    fn rows(&self, sql: &str, id: i64) -> rusqlite::Result<Vec<Value>> {
        let mut stmt = self.conn.prepare(sql)?;
        let n = stmt.column_count();
        let rows = stmt.query_map([id], |r| {
            Ok(Value::Array((0..n).map(|i| match r.get_ref(i) {
                Ok(ValueRef::Integer(v)) => json!(v),
                Ok(ValueRef::Real(v)) => json!(v),
                Ok(ValueRef::Text(t)) => json!(String::from_utf8_lossy(t)),
                _ => Value::Null,
            }).collect()))
        })?;
        rows.collect()
    }

    fn export(&self, kind: &str, r: &Row) -> rusqlite::Result<Value> {
        let id: Option<i64> = r.get("id").ok();
        let c = |name: &str| cell(r, name);
        let fk = |name: &str| r.get::<_, Option<i64>>(name).ok().flatten();
        Ok(match kind {
            "course" => json!({
                "name": c("name"), "teacher": c("teacher"), "room": c("room"), "color": c("color"),
                "external_id": c("external_id"),
                "slots": self.rows("SELECT weekday, start_min, end_min, room FROM class_slots WHERE course_id = ? ORDER BY weekday, start_min", id.unwrap_or(-1))?,
            }),
            "job" => json!({
                "name": c("name"), "color": c("color"), "hourly_rate": c("hourly_rate"), "deductions": c("deductions"),
                "pay_mode": c("pay_mode"), "monthly_pay": c("monthly_pay"), "mensilities": c("mensilities"),
                "contract_start": c("contract_start"), "contract_end": c("contract_end"), "tax_model": c("tax_model"),
                "inps": c("inps"), "addizionali": c("addizionali"), "fixed_term": flag(r, "fixed_term"),
                "cuneo": flag(r, "cuneo"), "payday": c("payday"),
                "pays_previous_month": flag(r, "pays_previous_month"), "payday_since": c("payday_since"),
                "patterns": self.rows("SELECT weekday, start_min, duration_min, break_min, since, until FROM shift_patterns WHERE job_id = ? ORDER BY weekday, start_min", id.unwrap_or(-1))?,
                "skips": self.rows("SELECT day, start_min FROM shift_skips WHERE job_id = ?", id.unwrap_or(-1))?,
            }),
            "event" => json!({"day": c("day"), "start": c("start_min"), "end": c("end_min"), "title": c("title"), "details": c("details"), "color": c("color")}),
            "routine" => json!({"title": c("title"), "start": c("start_min"), "color": c("color"),
                "weekdays": number_list(c("weekdays").as_str().unwrap_or(""))}),
            "task" => json!({"title": c("title"), "kind": c("kind"), "course": self.uid("courses", fk("course_id")), "due": c("due"),
                "done": flag(r, "done"), "details": c("details"), "external_id": c("external_id")}),
            "notebook" => json!({"name": c("name"), "color": c("color")}),
            "note" => json!({"title": c("title"), "body": c("body"), "course": self.uid("courses", fk("course_id")),
                "pinned": flag(r, "pinned"), "updated": c("updated"), "topic": c("topic"),
                "notebook": self.uid("notebooks", fk("notebook_id"))}),
            "image" => json!({"mime": c("mime"), "data": c("data")}),
            "journal" => json!({"day": c("day"), "body": c("body")}),
            "water" => json!({"day": c("day"), "at": c("at_min"), "ml": c("ml")}),
            "medicine" => json!({"name": c("name"), "dose": c("dose"), "notes": c("notes"), "active": flag(r, "active"),
                "times": number_list(c("times").as_str().unwrap_or("")), "weekdays": number_list(c("weekdays").as_str().unwrap_or(""))}),
            "dose" => json!({"medicine": self.uid("medicines", fk("medicine_id")), "day": c("day"), "planned": c("planned"),
                "taken": flag(r, "taken"), "at": c("at_min")}),
            "money" => json!({"day": c("day"), "cents": c("cents"), "income": flag(r, "income"), "category": c("category"), "note": c("note")}),
            "sleep" => json!({"day": c("day"), "asleep": c("asleep"), "woke": c("woke"), "quality": c("quality"), "note": c("note")}),
            "shift" => json!({"job": self.uid("jobs", fk("job_id")), "day": c("day"), "start": c("start_min"), "duration": c("duration_min"),
                "break": c("break_min"), "notes": c("notes")}),
            "focus" => json!({"started": c("started"), "minutes": c("minutes"), "task": self.uid("tasks", fk("task_id")),
                "label": c("label"), "complete": flag(r, "complete")}),
            other => unreachable!("not a synced kind: {other}"),
        })
    }

    // ---- incoming ---------------------------------------------------------------------------------------------------

    fn local_modified(&self, s: &Synced, uid: &str) -> rusqlite::Result<Option<String>> {
        let row: Option<Option<String>> =
            self.conn.query_row(&format!("SELECT modified FROM {} WHERE uid = ?", s.table), [uid], |r| r.get(0)).optional()?;
        if let Some(found) = row {
            return Ok(found);
        }
        self.conn
            .query_row("SELECT modified FROM sync_tombstones WHERE kind = ? AND uid = ?", params![s.kind, uid], |r| r.get(0))
            .optional()
    }

    /// Applies records from elsewhere; returns the (kind, uid) of what changed here.
    pub fn apply(&self, records: &[SyncRecord]) -> rusqlite::Result<HashSet<(String, String)>> {
        self.applying(|conn| {
            let mut changed = HashSet::new();
            let waiting: Vec<SyncRecord> = {
                let mut stmt = conn.prepare("SELECT record FROM sync_inbox")?;
                let texts = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                texts.iter().filter_map(|t| load(t)).collect()
            };
            conn.execute_batch("DELETE FROM sync_inbox")?;
            // Newest version of each record only, parents first.
            let mut latest: HashMap<(String, String), SyncRecord> = HashMap::new();
            let mut order: Vec<(String, String)> = Vec::new();
            for record in waiting.iter().chain(records) {
                if by_kind(&record.kind).is_none() {
                    continue;
                }
                match latest.get(&record.key()) {
                    Some(known) if record.modified <= known.modified => {}
                    Some(_) => {
                        latest.insert(record.key(), record.clone());
                    }
                    None => {
                        order.push(record.key());
                        latest.insert(record.key(), record.clone());
                    }
                }
            }
            let mut sorted: Vec<&SyncRecord> = order.iter().map(|k| &latest[k]).collect();
            sorted.sort_by_key(|r| by_kind(&r.kind).map_or(usize::MAX, |(i, _)| i));
            for record in sorted {
                let (_, s) = by_kind(&record.kind).expect("checked above");
                if let Some(local) = self.local_modified(s, &record.uid)? {
                    if local >= record.modified {
                        continue; // Ours is as new or newer: it wins (and gets sent if dirty).
                    }
                }
                if record.deleted {
                    conn.execute(&format!("DELETE FROM {} WHERE uid = ?", s.table), [&record.uid])?;
                    conn.execute("INSERT OR REPLACE INTO sync_tombstones VALUES (?, ?, ?, 0)", params![record.kind, record.uid, record.modified])?;
                    changed.insert(record.key());
                } else if self.upsert(s, record)? {
                    conn.execute("DELETE FROM sync_tombstones WHERE kind = ? AND uid = ?", params![record.kind, record.uid])?;
                    changed.insert(record.key());
                } else {
                    conn.execute("INSERT OR REPLACE INTO sync_inbox VALUES (?, ?, ?)", params![record.kind, record.uid, dump(record)])?;
                }
            }
            Ok(changed)
        })
    }

    /// Writes one record; false if a record it belongs to hasn't arrived yet.
    fn upsert(&self, s: &Synced, record: &SyncRecord) -> rusqlite::Result<bool> {
        let empty = Map::new();
        let d = record.data.as_ref().and_then(Value::as_object).unwrap_or(&empty);
        let get = |k: &str| d.get(k).cloned().unwrap_or(Value::Null);
        let or = |k: &str, default: Value| d.get(k).cloned().unwrap_or(default);
        let mut values: Vec<(&str, Value)> = match s.kind {
            "course" => vec![("name", get("name")), ("teacher", or("teacher", json!(""))), ("room", or("room", json!(""))),
                ("color", or("color", json!("#4f7cff"))), ("external_id", get("external_id"))],
            "job" => vec![("name", get("name")), ("color", or("color", json!("#0090ff"))), ("hourly_rate", get("hourly_rate")),
                ("deductions", or("deductions", json!(0))), ("pay_mode", or("pay_mode", json!("hourly"))),
                ("monthly_pay", get("monthly_pay")), ("mensilities", or("mensilities", json!(13))),
                ("contract_start", get("contract_start")), ("contract_end", get("contract_end")),
                ("tax_model", or("tax_model", json!("flat"))), ("inps", or("inps", json!(9.19))),
                ("addizionali", or("addizionali", json!(0))),
                ("fixed_term", json!(i64::from(d.get("fixed_term").is_none_or(|v| truthy(Some(v)))))),
                ("cuneo", json!(i64::from(truthy(d.get("cuneo"))))), ("payday", get("payday")),
                ("pays_previous_month", json!(i64::from(d.get("pays_previous_month").is_none_or(|v| truthy(Some(v)))))),
                ("payday_since", get("payday_since"))],
            "event" => vec![("day", get("day")), ("start_min", get("start")), ("end_min", get("end")), ("title", get("title")),
                ("details", or("details", json!(""))), ("color", or("color", json!("#8a8f98")))],
            "routine" => vec![("title", get("title")), ("start_min", get("start")),
                ("weekdays", json!(joined(d.get("weekdays"), &[0, 1, 2, 3, 4, 5, 6]))), ("color", or("color", json!("#f5a524")))],
            "task" => vec![("title", get("title")), ("kind", or("kind", json!("task"))), ("course_id", self.id("courses", d.get("course"))),
                ("due", get("due")), ("done", json!(i64::from(truthy(d.get("done"))))), ("details", or("details", json!(""))),
                ("external_id", get("external_id"))],
            "notebook" => vec![("name", get("name")), ("color", or("color", json!("#8a8f98")))],
            "note" => vec![("title", or("title", json!("Untitled"))), ("body", or("body", json!(""))),
                ("course_id", self.id("courses", d.get("course"))), ("pinned", json!(i64::from(truthy(d.get("pinned"))))),
                ("updated", get("updated")), ("topic", or("topic", json!(""))),
                // The phone may set a class on a note that's in a notebook: the class wins.
                ("notebook_id", if truthy(d.get("course")) { Value::Null } else { self.id("notebooks", d.get("notebook")) })],
            "image" => vec![("mime", get("mime"))],
            "journal" => vec![("day", get("day")), ("body", or("body", json!("")))],
            "water" => vec![("day", get("day")), ("at_min", get("at")), ("ml", get("ml"))],
            "medicine" => vec![("name", get("name")), ("dose", or("dose", json!(""))), ("notes", or("notes", json!(""))),
                ("active", json!(i64::from(d.get("active").is_none_or(|v| truthy(Some(v)))))),
                ("times", json!(joined(d.get("times"), &[]))), ("weekdays", json!(joined(d.get("weekdays"), &[])))],
            "dose" => {
                let medicine = self.id("medicines", d.get("medicine"));
                if medicine.is_null() {
                    return Ok(false); // Its medicine comes later.
                }
                vec![("medicine_id", medicine), ("day", get("day")), ("planned", get("planned")),
                    ("taken", json!(i64::from(d.get("taken").is_none_or(|v| truthy(Some(v)))))), ("at_min", or("at", json!(0)))]
            }
            "money" => vec![("day", get("day")), ("cents", get("cents")), ("income", json!(i64::from(truthy(d.get("income"))))),
                ("category", or("category", json!(""))), ("note", or("note", json!("")))],
            "sleep" => vec![("day", get("day")), ("asleep", get("asleep")), ("woke", get("woke")), ("quality", get("quality")),
                ("note", or("note", json!("")))],
            "shift" => {
                let job = self.id("jobs", d.get("job"));
                if job.is_null() {
                    return Ok(false); // Its job comes later.
                }
                vec![("job_id", job), ("day", get("day")), ("start_min", get("start")), ("duration_min", get("duration")),
                    ("break_min", or("break", json!(0))), ("notes", or("notes", json!("")))]
            }
            // Older apps don't send "complete": everything they logged was finished.
            "focus" => vec![("started", get("started")), ("minutes", get("minutes")), ("task_id", self.id("tasks", d.get("task"))),
                ("label", or("label", json!(""))), ("complete", json!(i64::from(d.get("complete").is_none_or(|v| truthy(Some(v))))))],
            other => unreachable!("not a synced kind: {other}"),
        };
        values.extend([("uid", json!(record.uid)), ("modified", json!(record.modified)), ("dirty", json!(0))]);
        let mut sql_values: Vec<Sql> = values.iter().map(|(_, v)| sql(v)).collect();
        let mut names: Vec<&str> = values.iter().map(|(k, _)| *k).collect();
        if s.kind == "image" {
            let data = d.get("data").and_then(Value::as_str).unwrap_or("");
            let bytes = base64::engine::general_purpose::STANDARD.decode(data).unwrap_or_default();
            names.insert(1, "data");
            sql_values.insert(1, Sql::Blob(bytes));
        }
        let mut row: Option<i64> =
            self.conn.query_row(&format!("SELECT rowid FROM {} WHERE uid = ?", s.table), [&record.uid], |r| r.get(0)).optional()?;
        if row.is_none() {
            row = self.same_thing(s, &values)?;
        }
        let rowid = match row {
            None => {
                let marks = vec!["?"; names.len()].join(", ");
                self.conn.execute(&format!("INSERT INTO {} ({}) VALUES ({marks})", s.table, names.join(", ")), rusqlite::params_from_iter(&sql_values))?;
                self.conn.last_insert_rowid()
            }
            Some(rowid) => {
                let sets = names.iter().map(|c| format!("{c} = ?")).collect::<Vec<_>>().join(", ");
                sql_values.push(Sql::Integer(rowid));
                self.conn.execute(&format!("UPDATE {} SET {sets} WHERE rowid = ?", s.table), rusqlite::params_from_iter(&sql_values))?;
                rowid
            }
        };
        let list = |k: &str| d.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        if s.kind == "course" {
            self.conn.execute("DELETE FROM class_slots WHERE course_id = ?", [rowid])?;
            for slot in list("slots") {
                let v: Vec<Sql> = std::iter::once(Sql::Integer(rowid)).chain(slot.as_array().into_iter().flatten().map(sql)).collect();
                self.conn.execute("INSERT INTO class_slots (course_id, weekday, start_min, end_min, room) VALUES (?, ?, ?, ?, ?)", rusqlite::params_from_iter(&v))?;
            }
        } else if s.kind == "job" {
            self.conn.execute("DELETE FROM shift_patterns WHERE job_id = ?", [rowid])?;
            for p in list("patterns") {
                let v: Vec<Sql> = std::iter::once(Sql::Integer(rowid)).chain(p.as_array().into_iter().flatten().map(sql)).collect();
                self.conn.execute(
                    "INSERT INTO shift_patterns (job_id, weekday, start_min, duration_min, break_min, since, until) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params_from_iter(&v),
                )?;
            }
            self.conn.execute("DELETE FROM shift_skips WHERE job_id = ?", [rowid])?;
            for k in list("skips") {
                let v: Vec<Sql> = std::iter::once(Sql::Integer(rowid)).chain(k.as_array().into_iter().flatten().map(sql)).collect();
                self.conn.execute("INSERT OR IGNORE INTO shift_skips (job_id, day, start_min) VALUES (?, ?, ?)", rusqlite::params_from_iter(&v))?;
            }
        }
        Ok(true)
    }

    /// A local row that is the same thing under another uid: the same day's journal, or the same register item
    /// imported before this device had synced.
    fn same_thing(&self, s: &Synced, values: &[(&str, Value)]) -> rusqlite::Result<Option<i64>> {
        let value = |k: &str| values.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone()).unwrap_or(Value::Null);
        if matches!(s.kind, "journal" | "sleep") {
            return self.conn.query_row(&format!("SELECT rowid FROM {} WHERE day = ?", s.table), [sql(&value("day"))], |r| r.get(0)).optional();
        }
        let external = value("external_id");
        if matches!(s.kind, "task" | "course") && truthy(Some(&external)) {
            return self.conn.query_row(&format!("SELECT rowid FROM {} WHERE external_id = ?", s.table), [sql(&external)], |r| r.get(0)).optional();
        }
        Ok(None)
    }

    // ---- bookkeeping --------------------------------------------------------------------------------------------------

    pub fn mark_sent(&self, records: &[SyncRecord]) -> rusqlite::Result<()> {
        self.applying(|conn| {
            for r in records {
                if r.deleted {
                    conn.execute("UPDATE sync_tombstones SET dirty = 0 WHERE kind = ? AND uid = ? AND modified = ?", params![r.kind, r.uid, r.modified])?;
                } else if let Some((_, s)) = by_kind(&r.kind) {
                    conn.execute(&format!("UPDATE {} SET dirty = 0 WHERE uid = ? AND modified = ?", s.table), params![r.uid, r.modified])?;
                }
            }
            Ok(())
        })
    }

    pub fn pending(&self) -> rusqlite::Result<i64> {
        let mut total: i64 = self.conn.query_row("SELECT COUNT(*) FROM sync_tombstones WHERE dirty = 1", [], |r| r.get(0))?;
        for s in SYNCED {
            total += self.conn.query_row(&format!("SELECT COUNT(*) FROM {} WHERE dirty = 1", s.table), [], |r| r.get::<_, i64>(0))?;
        }
        Ok(total)
    }

    /// Everything goes out again (after moving to another sync backend). Tracking is off: `modified` stays as it is.
    pub fn mark_all_dirty(&self) -> rusqlite::Result<()> {
        self.applying(|conn| {
            for s in SYNCED {
                conn.execute_batch(&format!("UPDATE {} SET dirty = 1", s.table))?;
            }
            conn.execute_batch("UPDATE sync_tombstones SET dirty = 1")
        })
    }

    pub fn clear_all(&self) -> rusqlite::Result<()> {
        self.applying(|conn| {
            for s in SYNCED.iter().rev() {
                conn.execute_batch(&format!("DELETE FROM {}", s.table))?;
            }
            conn.execute_batch("DELETE FROM sync_tombstones; DELETE FROM sync_inbox")
        })
    }
}
