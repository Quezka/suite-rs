//! The self-hosted Quire server (port of `infrastructure/quire_server.py`; protocol in quire-server/docs/PROTOCOL.md).

use std::sync::Arc;

use serde_json::{json, Value};

use super::firebase::batches;
use super::http::{urlencode, Http};
use crate::application::errors::{AppError, AppResult};
use crate::application::ports::{CloudBackend, CloudConfig, CloudSession, SyncRecord};

pub const BATCH: usize = 400;
pub const BATCH_BYTES: usize = 8_000_000;
const EXPIRED: [&str; 4] = ["invalid_refresh_token", "user_not_found", "invalid_token", "missing_token"];
const UNREADABLE: &str = "The sync server sent an answer Quire doesn't understand.";

/// The server's address from the settings: `https://` is assumed when none is typed.
pub fn base_url(c: &CloudConfig) -> String {
    let url = c.project_id.trim().trim_end_matches('/');
    if url.contains("://") { url.into() } else { format!("https://{url}") }
}

pub fn to_json(r: &SyncRecord) -> Value {
    json!({"kind": r.kind, "uid": r.uid, "modified": r.modified, "deleted": r.deleted,
           "data": if r.deleted { Value::Null } else { r.data.clone().unwrap_or(Value::Null) }})
}

pub fn from_json(raw: &Value) -> Option<SyncRecord> {
    let deleted = raw.get("deleted").and_then(Value::as_bool).unwrap_or(false);
    let data = raw.get("data").cloned().filter(Value::is_object);
    if !deleted && data.is_none() {
        return None;
    }
    let s = |k: &str| raw.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
    Some(SyncRecord { kind: s("kind")?, uid: s("uid")?, modified: s("modified")?, deleted, data: if deleted { None } else { data } })
}

/// The server said no: (status, error object).
struct Refused(u16, Value);

pub struct QuireServerCloud {
    pub http: Arc<dyn Http>,
    pub device: String,
    pub version: String,
}

impl QuireServerCloud {
    fn call(&self, c: &CloudConfig, method: &str, path: &str, body: Option<Value>, query: Option<(&str, &str)>, token: Option<&str>)
            -> Result<AppResult<Value>, Refused> {
        let mut url = base_url(c) + path;
        if let Some((k, v)) = query {
            url = format!("{url}?{k}={}", urlencode(v));
        }
        let mut headers = vec![("Accept", "application/json".to_string()), ("User-Agent", format!("Quire/{}", self.version))];
        if body.is_some() {
            headers.push(("Content-Type", "application/json".into()));
        }
        if let Some(t) = token {
            headers.push(("Authorization", format!("Bearer {t}")));
        }
        let (status, text) = match self.http.send(method, &url, &headers, body.map(|b| b.to_string())) {
            Ok(r) => r,
            Err(_) => return Ok(Err(AppError::Sync("Can't reach the sync server. Check your internet connection.".into()))),
        };
        let parsed: Option<Value> = if text.trim().is_empty() { Some(Value::Null) } else { serde_json::from_str(&text).ok() };
        if (200..300).contains(&status) {
            return Ok(parsed.ok_or_else(|| AppError::Sync(UNREADABLE.into())));
        }
        let error = parsed.and_then(|v| v.get("error").cloned()).filter(Value::is_object).unwrap_or(json!({}));
        Err(Refused(status, error))
    }

    fn error(r: &Refused, account: bool) -> AppError {
        let message = r.1.get("message").and_then(Value::as_str).unwrap_or("").to_string();
        let code = r.1.get("code").and_then(Value::as_str).unwrap_or("");
        if EXPIRED.contains(&code) {
            return AppError::CloudAuth("Your sign-in has expired: set up sync again.".into());
        }
        if r.0 == 429 && !message.is_empty() {
            return AppError::Sync(message); // "wait N seconds": not a dead sign-in
        }
        if account && (400..500).contains(&r.0) && !message.is_empty() {
            return AppError::CloudAuth(message);
        }
        if r.0 == 401 {
            return AppError::CloudAuth("Your sign-in has expired: set up sync again.".into());
        }
        AppError::Sync(format!("The sync server answered with an error ({}).", r.0))
    }

    fn request(&self, c: &CloudConfig, method: &str, path: &str, body: Option<Value>, account: bool) -> AppResult<Value> {
        match self.call(c, method, path, body, None, None) {
            Ok(r) => r,
            Err(refused) => Err(Self::error(&refused, account)),
        }
    }

    /// A signed-in call; if the access token ran out (they last an hour), refresh and try once more.
    fn authed(&self, c: &CloudConfig, s: &CloudSession, method: &str, path: &str, body: Option<Value>, query: Option<(&str, &str)>) -> AppResult<Value> {
        match self.call(c, method, path, body.clone(), query, Some(&s.token)) {
            Ok(r) => return r,
            Err(refused) if refused.1.get("code").and_then(Value::as_str) != Some("token_expired") => return Err(Self::error(&refused, false)),
            Err(_) => {}
        }
        let fresh = self.refresh(c, &s.refresh_token)?;
        match self.call(c, method, path, body, query, Some(&fresh.token)) {
            Ok(r) => r,
            Err(refused) => Err(Self::error(&refused, false)),
        }
    }

    fn session(answer: &Value, email: &str) -> AppResult<CloudSession> {
        let get = |k: &str| answer.get(k).and_then(Value::as_str).map(str::to_string);
        Ok(CloudSession {
            user_id: get("user_id").ok_or_else(|| AppError::Sync(UNREADABLE.into()))?,
            email: get("email").filter(|e| !e.is_empty()).unwrap_or_else(|| email.into()),
            token: get("token").ok_or_else(|| AppError::Sync(UNREADABLE.into()))?,
            refresh_token: get("refresh_token").ok_or_else(|| AppError::Sync(UNREADABLE.into()))?,
        })
    }
}

impl CloudBackend for QuireServerCloud {
    fn sign_up(&self, c: &CloudConfig, email: &str, password: &str, invite: &str) -> AppResult<CloudSession> {
        let mut body = json!({"email": email, "password": password, "device": self.device});
        if !invite.is_empty() {
            body["invite"] = json!(invite);
        }
        Self::session(&self.request(c, "POST", "/v1/signup", Some(body), true)?, email)
    }

    fn sign_in(&self, c: &CloudConfig, email: &str, password: &str) -> AppResult<CloudSession> {
        let body = json!({"email": email, "password": password, "device": self.device});
        Self::session(&self.request(c, "POST", "/v1/signin", Some(body), true)?, email)
    }

    fn refresh(&self, c: &CloudConfig, refresh_token: &str) -> AppResult<CloudSession> {
        let mut answer = self.request(c, "POST", "/v1/refresh", Some(json!({"refresh_token": refresh_token})), false)?;
        // the refresh token comes back unchanged; keep ours if an older server leaves it out
        if answer.get("refresh_token").is_none() && answer.is_object() {
            answer["refresh_token"] = json!(refresh_token);
        }
        Self::session(&answer, "")
    }

    fn pull(&self, c: &CloudConfig, s: &CloudSession, cursor: Option<&str>) -> AppResult<(Vec<SyncRecord>, Option<String>)> {
        let mut cursor = cursor.map(str::to_string);
        let mut records = Vec::new();
        loop {
            let answer = self.authed(c, s, "GET", "/v1/records", None, cursor.as_deref().map(|c| ("after", c)))?;
            let rows = answer.get("records").and_then(Value::as_array).ok_or_else(|| AppError::Sync(UNREADABLE.into()))?;
            let more = answer.get("more").and_then(Value::as_bool).ok_or_else(|| AppError::Sync(UNREADABLE.into()))?;
            if let Some(next) = answer.get("cursor").and_then(Value::as_str).filter(|c| !c.is_empty()) {
                cursor = Some(next.into());
            }
            records.extend(rows.iter().filter_map(from_json));
            if !more {
                return Ok((records, cursor));
            }
        }
    }

    fn push(&self, c: &CloudConfig, s: &CloudSession, records: &[SyncRecord]) -> AppResult {
        for batch in batches(records, BATCH, BATCH_BYTES) {
            self.authed(c, s, "POST", "/v1/records", Some(json!({"records": batch.iter().map(to_json).collect::<Vec<_>>()})), None)?;
        }
        Ok(())
    }
}
