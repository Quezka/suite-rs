//! The cloud backends against a fake HTTP: request shapes and answers as the Python adapters' tests expect.

use std::sync::{Arc, Mutex};

use quire::application::errors::AppError;
use quire::application::ports::*;
use quire::infrastructure::firebase::{from_fields, to_fields, FirebaseCloud};
use quire::infrastructure::http::Http;
use quire::infrastructure::quire_server::QuireServerCloud;
use serde_json::{json, Value};

type Call = (String, String, Option<String>);

struct Fake {
    calls: Mutex<Vec<Call>>,
    answers: Mutex<Vec<(u16, String)>>,
}

impl Fake {
    fn new(answers: Vec<(u16, Value)>) -> Arc<Self> {
        Arc::new(Self { calls: Mutex::new(Vec::new()), answers: Mutex::new(answers.into_iter().rev().map(|(s, v)| (s, v.to_string())).collect()) })
    }
}

impl Http for Fake {
    fn send(&self, method: &str, url: &str, _h: &[(&str, String)], body: Option<String>) -> Result<(u16, String), String> {
        self.calls.lock().unwrap().push((method.into(), url.into(), body));
        Ok(self.answers.lock().unwrap().pop().unwrap_or((500, String::new())))
    }
}

fn record() -> SyncRecord {
    SyncRecord { kind: "task".into(), uid: "u1".into(), modified: "2026-11-16T10:00:00.000Z".into(), deleted: false, data: Some(json!({"title": "Lab"})) }
}

#[test]
fn firestore_fields_round_trip_and_sign_in_errors_read_well() {
    assert_eq!(from_fields(&to_fields(&record())), Some(record()));
    let http = Fake::new(vec![(400, json!({"error": {"message": "INVALID_LOGIN_CREDENTIALS"}}))]);
    let fb = FirebaseCloud { http: http.clone() };
    let c = CloudConfig { project_id: "quire-x1".into(), api_key: "k".into(), backend: FIREBASE.into() };
    assert_eq!(fb.sign_in(&c, "me@x.it", "bad"), Err(AppError::CloudAuth("Wrong email or password.".into())));
    assert!(http.calls.lock().unwrap()[0].1.contains("accounts:signInWithPassword?key=k"));
}

#[test]
fn firestore_pull_follows_the_cursor() {
    let doc = json!([{"document": {"name": "projects/p/doc1", "fields": {"synced": {"timestampValue": "2026-11-16T10:00:00Z"},
                     "kind": {"stringValue": "task"}, "uid": {"stringValue": "u1"}, "modified": {"stringValue": "m"},
                     "deleted": {"booleanValue": false}, "data": {"stringValue": "{\"title\":\"Lab\"}"}}}}]);
    let http = Fake::new(vec![(200, doc)]);
    let fb = FirebaseCloud { http: http.clone() };
    let c = CloudConfig { project_id: "quire-x1".into(), api_key: "k".into(), backend: FIREBASE.into() };
    let s = CloudSession { user_id: "uid9".into(), email: String::new(), token: "t".into(), refresh_token: "r".into() };
    let (records, cursor) = fb.pull(&c, &s, None).unwrap();
    assert_eq!(records[0].data, Some(json!({"title": "Lab"})));
    assert_eq!(cursor.as_deref(), Some("[\"2026-11-16T10:00:00Z\", \"projects/p/doc1\"]"));
    assert!(http.calls.lock().unwrap()[0].1.ends_with("/users/uid9:runQuery"));
}

#[test]
fn quire_server_refreshes_an_expired_token_once_and_pages() {
    let http = Fake::new(vec![
        (401, json!({"error": {"code": "token_expired"}})),
        (200, json!({"user_id": "1", "token": "t2"})),
        (200, json!({"records": [{"kind": "task", "uid": "u1", "modified": "m", "deleted": false, "data": {"title": "Lab"}}], "more": true, "cursor": "c1"})),
        (200, json!({"records": [{"kind": "task", "uid": "u2", "modified": "m", "deleted": true}], "more": false})),
    ]);
    let server = QuireServerCloud { http: http.clone(), device: "test".into(), version: "0".into() };
    let c = CloudConfig { project_id: "sync.example.com/".into(), api_key: String::new(), backend: SERVER.into() };
    let s = CloudSession { user_id: "1".into(), email: String::new(), token: "t1".into(), refresh_token: "r".into() };
    let (records, cursor) = server.pull(&c, &s, None).unwrap();
    assert_eq!((records.len(), records[1].deleted, cursor.as_deref()), (2, true, Some("c1")));
    let calls = http.calls.lock().unwrap();
    assert_eq!(calls[1].1, "https://sync.example.com/v1/refresh");
    assert_eq!(calls[3].1, "https://sync.example.com/v1/records?after=c1");
    drop(calls);
    let bad = Fake::new(vec![(403, json!({"error": {"message": "Sign-up is closed."}}))]);
    let server = QuireServerCloud { http: bad, device: "test".into(), version: "0".into() };
    assert_eq!(server.sign_up(&c, "me@x.it", "pw", ""), Err(AppError::CloudAuth("Sign-up is closed.".into())));
}
