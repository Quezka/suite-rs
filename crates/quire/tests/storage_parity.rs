//! The Rust storage and sync behave like the Python app's (fixtures from `tools/quire_fixtures.py`): it opens a
//! database the Python app made without changing its layout, sends out the same records, and applying the phone's
//! records gives the same tables.

use quire::application::ports::SyncRecord;
use quire::infrastructure::sqlite::Database;
use quire::infrastructure::sync_store::{SyncStore, SYNCED};
use rusqlite::types::ValueRef;
use serde_json::{json, Value};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/storage.json")).unwrap()
}

fn record(v: &Value) -> SyncRecord {
    SyncRecord {
        kind: v["kind"].as_str().unwrap().into(),
        uid: v["uid"].as_str().unwrap().into(),
        modified: v["modified"].as_str().unwrap().into(),
        deleted: v["deleted"].as_bool().unwrap(),
        data: v.get("data").filter(|d| !d.is_null()).cloned(),
    }
}

fn as_json(r: &SyncRecord) -> String {
    json!({"kind": r.kind, "uid": r.uid, "modified": r.modified, "deleted": r.deleted, "data": r.data}).to_string()
}

fn python_db_copy() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("quire-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("python.db");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/python.db"), &path).unwrap();
    path
}

#[test]
fn opens_a_python_database_without_changing_its_layout() {
    let db = Database::open(python_db_copy()).unwrap();
    let mut stmt = db.conn.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL").unwrap();
    let mut schema: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    schema.sort();
    let want: Vec<String> = fixtures()["schema"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
    assert_eq!(schema, want);
    let version: i32 = db.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 18);
}

#[test]
fn a_fresh_database_has_the_python_layout() {
    let db = Database::in_memory().unwrap();
    let mut stmt = db.conn.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL").unwrap();
    let mut schema: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    schema.sort();
    let want: Vec<String> = fixtures()["schema"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
    assert_eq!(schema, want);
}

#[test]
fn sends_the_same_records() {
    let db = Database::open(python_db_copy()).unwrap();
    let mut ours: Vec<String> = SyncStore::new(&db.conn).outgoing().unwrap().iter().map(as_json).collect();
    let mut theirs: Vec<String> = fixtures()["outgoing"].as_array().unwrap().iter().map(|v| as_json(&record(v))).collect();
    ours.sort();
    theirs.sort();
    assert_eq!(ours.len(), theirs.len());
    for (a, b) in ours.iter().zip(&theirs) {
        assert_eq!(serde_json::from_str::<Value>(a).unwrap(), serde_json::from_str::<Value>(b).unwrap());
    }
}

fn table(db: &Database, sql: &str) -> Vec<Value> {
    let mut stmt = db.conn.prepare(sql).unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |r| {
        Ok(Value::Array(
            (0..n)
                .map(|i| match r.get_ref(i).unwrap() {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => json!(v),
                    ValueRef::Real(v) => json!(v),
                    ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    ValueRef::Blob(b) => json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
                })
                .collect(),
        ))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn applying_the_phones_records_gives_the_same_tables() {
    let all = fixtures();
    let records: Vec<SyncRecord> = all["records"].as_array().unwrap().iter().map(record).collect();
    let db = Database::in_memory().unwrap();
    SyncStore::new(&db.conn).apply(&records).unwrap();
    let want = &all["applied"];
    for s in SYNCED {
        let ours = table(&db, &format!("SELECT * FROM {} ORDER BY rowid", s.table));
        assert_eq!(Value::Array(ours), want[s.table], "table {}", s.table);
    }
    for child in ["class_slots", "shift_patterns", "shift_skips"] {
        assert_eq!(Value::Array(table(&db, &format!("SELECT * FROM {child} ORDER BY rowid"))), want[child], "table {child}");
    }
    assert_eq!(Value::Array(table(&db, "SELECT kind, uid FROM sync_inbox ORDER BY kind, uid")), want["sync_inbox"]);
    // Applied changes aren't echoed back out.
    assert_eq!(SyncStore::new(&db.conn).pending().unwrap(), 0);
}

#[test]
fn local_changes_are_tracked_and_deletions_leave_tombstones() {
    let db = Database::in_memory().unwrap();
    let store = SyncStore::new(&db.conn);
    db.conn.execute("INSERT INTO events (day, start_min, end_min, title) VALUES ('2026-11-09', 540, 600, 'Maths')", []).unwrap();
    let out = store.outgoing().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["title"], "Maths");
    store.mark_sent(&out).unwrap();
    assert_eq!(store.pending().unwrap(), 0);
    db.conn.execute("DELETE FROM events", []).unwrap();
    let out = store.outgoing().unwrap();
    assert!(out[0].deleted && out[0].uid == out[0].uid);
    // A newer version from elsewhere wins over the tombstone; an older one doesn't.
    let mut back = SyncRecord { deleted: false, data: Some(json!({"day": "2026-11-09", "start": 540, "end": 600, "title": "Maths"})), ..out[0].clone() };
    back.modified = "2000-01-01T00:00:00.000Z".into();
    assert!(store.apply(&[back.clone()]).unwrap().is_empty());
    back.modified = "2999-01-01T00:00:00.000Z".into();
    assert_eq!(store.apply(&[back]).unwrap().len(), 1);
    let count: i64 = db.conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}
