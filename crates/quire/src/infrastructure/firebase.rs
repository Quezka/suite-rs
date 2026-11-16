//! Firebase: email/password accounts (Identity Toolkit) and Cloud Firestore through their REST APIs (port of
//! `infrastructure/firebase.py`). Records live in `users/<account>/records/<kind>~<uid>`; every write stamps a server
//! time (`synced`), and pulling asks for documents after the last one seen in (synced, name) order.

use std::sync::Arc;

use serde_json::{json, Value};

use super::http::{urlencode, Http};
use crate::application::errors::{AppError, AppResult};
use crate::application::ports::{CloudBackend, CloudConfig, CloudSession, SyncRecord};

pub const FIRESTORE: &str = "https://firestore.googleapis.com/v1";
pub const PAGE: usize = 300;
pub const BATCH: usize = 400; // Firestore allows 500 writes per commit
pub const BATCH_BYTES: usize = 6_000_000; // and 10 MiB per request

fn auth_message(code: &str) -> String {
    let base = code.split(':').next().unwrap_or("").trim();
    let known = match base {
        "EMAIL_EXISTS" => "There's already an account with this email: sign in instead.",
        "EMAIL_NOT_FOUND" => "No account with this email: create one first.",
        "INVALID_PASSWORD" | "INVALID_LOGIN_CREDENTIALS" => "Wrong email or password.",
        "INVALID_EMAIL" => "That email address doesn't look right.",
        "MISSING_PASSWORD" => "Type a password.",
        "USER_DISABLED" => "This account has been disabled in the Firebase console.",
        "OPERATION_NOT_ALLOWED" => "Turn on Email/Password sign-in in the Firebase console (Authentication → Sign-in method).",
        "TOO_MANY_ATTEMPTS_TRY_LATER" => "Too many attempts: wait a few minutes and try again.",
        "TOKEN_EXPIRED" | "INVALID_REFRESH_TOKEN" => "Your sign-in has expired: set up sync again.",
        "USER_NOT_FOUND" => "This account no longer exists: set up sync again.",
        "WEAK_PASSWORD" => "Choose a password of at least 6 characters.",
        b if b.starts_with("API key not valid") || b == "API_KEY_INVALID" =>
            "The Web API key isn't right: copy it again from the Firebase console.",
        _ => return format!("Firebase refused the sign-in ({base})."),
    };
    known.to_string()
}

pub fn doc_id(r: &SyncRecord) -> String {
    format!("{}~{}", r.kind, r.uid).replace('/', "%2F")
}

pub fn to_fields(r: &SyncRecord) -> Value {
    json!({
        "kind": {"stringValue": r.kind}, "uid": {"stringValue": r.uid}, "modified": {"stringValue": r.modified},
        "deleted": {"booleanValue": r.deleted},
        // one JSON string: the payload's shape can change without touching the schema
        "data": match &r.data { Some(d) => json!({"stringValue": d.to_string()}), None => json!({"nullValue": null}) },
    })
}

pub fn from_fields(f: &Value) -> Option<SyncRecord> {
    let s = |k: &str| f.get(k)?.get("stringValue")?.as_str().map(str::to_string);
    let data = match f.get("data").and_then(|d| d.get("stringValue")).and_then(Value::as_str) {
        Some(text) => Some(serde_json::from_str(text).ok()?),
        None => None,
    };
    Some(SyncRecord { kind: s("kind")?, uid: s("uid")?, modified: s("modified")?,
                      deleted: f.get("deleted").and_then(|d| d.get("booleanValue")).and_then(Value::as_bool).unwrap_or(false), data })
}

/// Groups small enough for one request: at most `max` records and about `bytes` of data (always at least one).
pub fn batches(records: &[SyncRecord], max: usize, bytes: usize) -> Vec<Vec<SyncRecord>> {
    let (mut out, mut batch, mut size): (Vec<Vec<SyncRecord>>, Vec<SyncRecord>, usize) = (Vec::new(), Vec::new(), 0);
    for r in records {
        let len = r.data.as_ref().map_or(0, |d| d.to_string().len());
        if !batch.is_empty() && (batch.len() >= max || size + len > bytes) {
            out.push(std::mem::take(&mut batch));
            size = 0;
        }
        batch.push(r.clone());
        size += len;
    }
    if !batch.is_empty() {
        out.push(batch);
    }
    out
}

pub struct FirebaseCloud {
    pub http: Arc<dyn Http>,
}

impl FirebaseCloud {
    fn post(&self, url: &str, body: String, form: bool, token: Option<&str>, auth: bool) -> AppResult<Value> {
        let mut headers = vec![("Content-Type", if form { "application/x-www-form-urlencoded" } else { "application/json" }.to_string())];
        if let Some(t) = token {
            headers.push(("Authorization", format!("Bearer {t}")));
        }
        let (status, text) = self.http.send("POST", url, &headers, Some(body))
            .map_err(|_| AppError::Sync("Can't reach the sync server. Check your internet connection.".into()))?;
        let parsed: Option<Value> = if text.trim().is_empty() { Some(Value::Null) } else { serde_json::from_str(&text).ok() };
        if (200..300).contains(&status) {
            return parsed.ok_or_else(|| AppError::Sync("The sync server sent an answer Quire doesn't understand.".into()));
        }
        let error = parsed.as_ref().and_then(|v| v.get("error")).cloned().unwrap_or(Value::Null);
        let message = error.get("message").and_then(Value::as_str).map(str::to_string)
            .unwrap_or_else(|| error.as_str().unwrap_or("").to_string());
        Err(if auth && (400..500).contains(&status) {
            AppError::CloudAuth(auth_message(&message))
        } else {
            match status {
                401 => AppError::CloudAuth("Your sign-in has expired: set up sync again.".into()),
                403 => AppError::Sync("Firestore refused access. Check that the database exists and that the security rules from the setup dialog are published.".into()),
                404 => AppError::Sync("Firestore database not found. Check the project ID and create the database in the Firebase console.".into()),
                _ => AppError::Sync(format!("The sync server answered with an error ({status}).")),
            }
        })
    }

    fn account(&self, action: &str, c: &CloudConfig, email: &str, password: &str) -> AppResult<CloudSession> {
        let url = format!("https://identitytoolkit.googleapis.com/v1/accounts:{action}?key={}", c.api_key);
        let r = self.post(&url, json!({"email": email, "password": password, "returnSecureToken": true}).to_string(), false, None, true)?;
        let get = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
        Ok(CloudSession { user_id: get("localId").unwrap_or_default(), email: get("email").unwrap_or_else(|| email.into()),
                          token: get("idToken").unwrap_or_default(), refresh_token: get("refreshToken").unwrap_or_default() })
    }

    fn root(c: &CloudConfig) -> String {
        format!("projects/{}/databases/(default)/documents", c.project_id)
    }
}

impl CloudBackend for FirebaseCloud {
    fn sign_up(&self, c: &CloudConfig, email: &str, password: &str, _invite: &str) -> AppResult<CloudSession> {
        self.account("signUp", c, email, password)
    }

    fn sign_in(&self, c: &CloudConfig, email: &str, password: &str) -> AppResult<CloudSession> {
        self.account("signInWithPassword", c, email, password)
    }

    fn refresh(&self, c: &CloudConfig, refresh_token: &str) -> AppResult<CloudSession> {
        let url = format!("https://securetoken.googleapis.com/v1/token?key={}", c.api_key);
        let body = format!("grant_type=refresh_token&refresh_token={}", urlencode(refresh_token));
        let r = self.post(&url, body, true, None, true)?;
        let get = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(CloudSession { user_id: get("user_id"), email: String::new(), token: get("id_token"), refresh_token: get("refresh_token") })
    }

    fn pull(&self, c: &CloudConfig, s: &CloudSession, cursor: Option<&str>) -> AppResult<(Vec<SyncRecord>, Option<String>)> {
        let parent = format!("{}/users/{}", Self::root(c), s.user_id);
        let mut cursor = cursor.map(str::to_string);
        let mut records = Vec::new();
        loop {
            let mut query = json!({
                "from": [{"collectionId": "records"}],
                "orderBy": [{"field": {"fieldPath": "synced"}, "direction": "ASCENDING"},
                            {"field": {"fieldPath": "__name__"}, "direction": "ASCENDING"}],
                "limit": PAGE,
            });
            if let Some(cur) = &cursor {
                if let Ok(Value::Array(pair)) = serde_json::from_str::<Value>(cur) {
                    query["startAt"] = json!({"values": [{"timestampValue": pair[0]}, {"referenceValue": pair[1]}], "before": false});
                }
            }
            let rows = self.post(&format!("{FIRESTORE}/{parent}:runQuery"), json!({"structuredQuery": query}).to_string(), false, Some(&s.token), false)?;
            let docs: Vec<&Value> = rows.as_array().map(|a| a.iter().filter_map(|r| r.get("document")).collect()).unwrap_or_default();
            for doc in &docs {
                let fields = doc.get("fields").cloned().unwrap_or(Value::Null);
                if let Some(r) = from_fields(&fields) {
                    records.push(r);
                }
                if let Some(synced) = fields.get("synced").and_then(|v| v.get("timestampValue")).and_then(Value::as_str) {
                    // same JSON spacing as Python's json.dumps: the cursor is shared through settings
                    cursor = Some(format!("[{}, {}]", Value::from(synced), doc.get("name").cloned().unwrap_or(Value::Null)));
                }
            }
            if docs.len() < PAGE {
                return Ok((records, cursor));
            }
        }
    }

    fn push(&self, c: &CloudConfig, s: &CloudSession, records: &[SyncRecord]) -> AppResult {
        let base = format!("{}/users/{}/records", Self::root(c), s.user_id);
        for batch in batches(records, BATCH, BATCH_BYTES) {
            let writes: Vec<Value> = batch.iter().map(|r| json!({
                "update": {"name": format!("{base}/{}", doc_id(r)), "fields": to_fields(r)},
                "updateTransforms": [{"fieldPath": "synced", "setToServerValue": "REQUEST_TIME"}],
            })).collect();
            self.post(&format!("{FIRESTORE}/{}:commit", Self::root(c)), json!({"writes": writes}).to_string(), false, Some(&s.token), false)?;
        }
        Ok(())
    }
}
