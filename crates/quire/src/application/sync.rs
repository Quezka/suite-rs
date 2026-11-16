//! Syncing Quire between devices through a cloud account (port of `services/sync.py`). Local-first; newer change wins.
//! Three steps so the network part can run on a worker thread: `prepare()` (UI) → `exchange()` (any thread) → `finish()` (UI).

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use base64::Engine;
use chrono::NaiveDateTime;
use fancy_regex::Regex;
use serde_json::json;

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::{Clock, CloudBackend, CloudConfig, CloudSession, CredentialStore, Credentials, KeyValueStore,
                   SyncRecord, SyncStorePort, FIREBASE, SERVER};
use crate::domain::ValidationError;

const PREFIX: &str = "sync.";
/// Change -> every device pulls all records once more (old apps dropped kinds they didn't know).
pub const FULL_PULL: &str = "v20-ideas";

pub fn topic_of(kind: &str) -> Option<Topic> {
    Some(match kind {
        "course" => Topic::Courses,
        "event" | "routine" => Topic::Events,
        "task" => Topic::Tasks,
        "note" | "image" | "notebook" => Topic::Notes,
        "journal" => Topic::Journal,
        "idea" => Topic::Ideas,
        "sleep" => Topic::Sleep,
        "water" => Topic::Water,
        "medicine" | "dose" => Topic::Medicine,
        "job" | "shift" => Topic::Work,
        "focus" => Topic::Focus,
        _ => return None,
    })
}

const ALL_TOPICS: [Topic; 11] = [Topic::Courses, Topic::Events, Topic::Tasks, Topic::Notes, Topic::Journal, Topic::Ideas,
                                 Topic::Sleep, Topic::Water, Topic::Medicine, Topic::Work, Topic::Focus];

#[derive(Clone, Debug, PartialEq)]
pub struct SyncStatus {
    pub set_up: bool,
    pub email: String,
    pub project_id: String,
    pub last_sync: Option<NaiveDateTime>,
    /// Local changes not sent yet.
    pub pending: i64,
    /// The last sync's error, "" if it worked.
    pub problem: String,
    pub backend: String,
}

/// What a sync needs from this device, gathered on the UI thread.
#[derive(Clone, Debug)]
pub struct SyncJob {
    pub config: CloudConfig,
    pub refresh_token: String,
    pub cursor: Option<String>,
    pub outgoing: Vec<SyncRecord>,
}

/// What came back from the cloud, to apply on the UI thread.
#[derive(Clone, Debug)]
pub struct SyncExchange {
    pub incoming: Vec<SyncRecord>,
    pub sent: Vec<SyncRecord>,
    pub cursor: Option<String>,
    pub refresh_token: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncResult {
    pub received: usize,
    pub sent: usize,
}

fn email_ok(email: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap()).is_match(email).unwrap_or(false)
}

fn project_ok(project: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9-]{4,28}[a-z0-9]$").unwrap()).is_match(project).unwrap_or(false)
}

pub struct SyncService {
    pub store: Rc<dyn SyncStorePort>,
    pub cloud: Arc<dyn CloudBackend>,
    pub secrets: Rc<dyn CredentialStore>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
}

impl SyncService {
    fn get(&self, name: &str) -> String {
        self.settings.get(&format!("{PREFIX}{name}")).unwrap_or_default()
    }

    fn set(&self, name: &str, value: Option<&str>) -> AppResult {
        self.settings.set(&format!("{PREFIX}{name}"), value)
    }

    pub fn config(&self) -> Option<CloudConfig> {
        let (project, key) = (self.get("project"), self.get("api_key"));
        if self.get("backend") == SERVER {
            return (!project.is_empty()).then(|| CloudConfig { project_id: project, api_key: String::new(), backend: SERVER.into() });
        }
        (!project.is_empty() && !key.is_empty()).then(|| CloudConfig { project_id: project, api_key: key, backend: FIREBASE.into() })
    }

    pub fn status(&self) -> AppResult<SyncStatus> {
        let backend = self.get("backend");
        Ok(SyncStatus {
            set_up: self.config().is_some() && self.secrets.load().is_some(), email: self.get("email"), project_id: self.get("project"),
            last_sync: NaiveDateTime::parse_from_str(&self.get("last_sync"), "%Y-%m-%dT%H:%M:%S").ok(),
            pending: self.store.pending()?, problem: self.get("problem"),
            backend: if backend.is_empty() { FIREBASE.into() } else { backend },
        })
    }

    /// The phone's setup code (shown as a QR code): carries secrets, so it's shown on screen, never sent.
    pub fn phone_link(&self, register: Option<&Credentials>) -> AppResult<String> {
        let (Some(config), Some(saved)) = (self.config(), self.secrets.load()) else {
            return Err(AppError::SyncNotSetUp("Set up sync on this computer first.".into()));
        };
        let mut data = if config.backend == SERVER {
            json!({"v": "2", "backend": SERVER, "url": config.project_id, "email": self.get("email"), "refresh": saved.password})
        } else {
            json!({"v": "1", "project": config.project_id, "api_key": config.api_key, "email": self.get("email"), "refresh": saved.password})
        };
        if let Some(r) = register {
            data["cv_user"] = json!(r.username);
            data["cv_pass"] = json!(r.password);
        }
        Ok(format!("quire-link:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data.to_string())))
    }

    pub fn check(project_id: &str, api_key: &str, email: &str, password: &str) -> AppResult<CloudConfig> {
        let (project_id, api_key, email) = (project_id.trim(), api_key.trim(), email.trim());
        if !project_ok(project_id) {
            return Err(ValidationError("Copy the project ID from the Firebase console, e.g. quire-sync-1a2b3.").into());
        }
        if api_key.len() < 20 || api_key.contains(' ') {
            return Err(ValidationError("Copy the Web API key from the Firebase console.").into());
        }
        if !email_ok(email) {
            return Err(ValidationError("Type the email address for your sync account.").into());
        }
        if password.is_empty() {
            return Err(ValidationError("Type a password.").into());
        }
        Ok(CloudConfig { project_id: project_id.into(), api_key: api_key.into(), backend: FIREBASE.into() })
    }

    /// The same checks for a Quire server: its address instead of a Firebase project.
    pub fn check_server(address: &str, email: &str, password: &str) -> AppResult<CloudConfig> {
        let address = address.trim();
        let full = if address.contains("://") { address.to_string() } else { format!("https://{address}") };
        let bad = || AppError::from(ValidationError("Type the address of your Quire server, e.g. https://sync.example.com."));
        let (scheme, rest) = full.split_once("://").ok_or_else(bad)?;
        let rest = rest.split(['?', '#']).next().unwrap_or("");
        let (netloc, path) = rest.split_once('/').map_or((rest, ""), |(n, p)| (n, p));
        let host = netloc.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
        if !(scheme == "http" || scheme == "https") || host.is_empty() || address.contains(' ') {
            return Err(bad());
        }
        if !email_ok(email.trim()) {
            return Err(ValidationError("Type the email address for your sync account.").into());
        }
        if password.is_empty() {
            return Err(ValidationError("Type a password.").into());
        }
        let path = if path.is_empty() { String::new() } else { format!("/{}", path.trim_end_matches('/')) };
        Ok(CloudConfig { project_id: format!("{scheme}://{netloc}{path}"), api_key: String::new(), backend: SERVER.into() })
    }

    /// Sign in (or create the account). Network only: run it on a worker thread.
    pub fn authenticate(cloud: &dyn CloudBackend, config: &CloudConfig, email: &str, password: &str, create: bool, invite: &str) -> AppResult<CloudSession> {
        if create {
            cloud.sign_up(config, email.trim(), password, invite.trim())
        } else {
            cloud.sign_in(config, email.trim(), password)
        }
    }

    /// Remember the account here. With `take_cloud_copy`, this device's data is replaced by the cloud's at the next sync.
    pub fn connect(&self, config: &CloudConfig, session: &CloudSession, take_cloud_copy: bool) -> AppResult {
        if take_cloud_copy {
            self.store.clear_all()?;
            for t in ALL_TOPICS {
                self.bus.publish(t);
            }
        }
        let backend = if self.get("backend").is_empty() { FIREBASE.to_string() } else { self.get("backend") };
        let moved = (backend, self.get("project")) != (config.backend.clone(), config.project_id.clone());
        if moved && !take_cloud_copy {
            self.store.mark_all_dirty()?; // a new place starts empty: send everything there
        }
        self.secrets.save(&Credentials { username: session.email.clone(), password: session.refresh_token.clone() })?;
        self.set("backend", Some(&config.backend))?;
        self.set("project", Some(&config.project_id))?;
        self.set("api_key", if config.api_key.is_empty() { None } else { Some(&config.api_key) })?;
        self.set("email", Some(&session.email))?;
        self.set("cursor", None)?; // pull everything once from the new account
        self.set("problem", None)?;
        if self.get("device").is_empty() {
            let id = format!("{:032x}", rand_u128());
            self.set("device", Some(&id))?;
        }
        Ok(())
    }

    /// Stop syncing on this device; its data stays here.
    pub fn disconnect(&self) -> AppResult {
        self.secrets.clear()?;
        for n in ["backend", "project", "api_key", "email", "cursor", "last_sync", "problem"] {
            self.set(n, None)?;
        }
        Ok(())
    }

    pub fn prepare(&self) -> AppResult<SyncJob> {
        let (Some(config), Some(saved)) = (self.config(), self.secrets.load()) else {
            return Err(AppError::SyncNotSetUp("Set up sync first.".into()));
        };
        // full pull once per FULL_PULL <- old app dropped kinds it didn't know (ideas before v20)
        let cursor = if self.get("full_pull") == FULL_PULL { Some(self.get("cursor")).filter(|c| !c.is_empty()) } else { None };
        Ok(SyncJob { config, refresh_token: saved.password, cursor, outgoing: self.store.outgoing()? })
    }

    /// Pull what changed elsewhere, then push what changed here and is still newest. Network only: any thread.
    pub fn exchange(cloud: &dyn CloudBackend, job: &SyncJob) -> AppResult<SyncExchange> {
        let session = cloud.refresh(&job.config, &job.refresh_token)?;
        let (incoming, cursor) = cloud.pull(&job.config, &session, job.cursor.as_deref())?;
        let mut newest: std::collections::HashMap<(String, String), String> = std::collections::HashMap::new();
        for r in &incoming {
            let e = newest.entry(r.key()).or_default();
            if r.modified > *e {
                *e = r.modified.clone();
            }
        }
        let sent: Vec<SyncRecord> = job.outgoing.iter()
            .filter(|r| r.modified >= newest.get(&r.key()).cloned().unwrap_or_default()).cloned().collect();
        if !sent.is_empty() {
            cloud.push(&job.config, &session, &sent)?;
        }
        Ok(SyncExchange { incoming, sent, cursor, refresh_token: session.refresh_token })
    }

    pub fn finish(&self, ex: &SyncExchange) -> AppResult<SyncResult> {
        let changed = self.store.apply(&ex.incoming)?;
        self.store.mark_sent(&ex.sent)?;
        self.set("cursor", ex.cursor.as_deref())?;
        self.set("full_pull", Some(FULL_PULL))?;
        self.set("last_sync", Some(&self.clock.now().format("%Y-%m-%dT%H:%M:%S").to_string()))?;
        self.set("problem", None)?;
        if let Some(saved) = self.secrets.load() {
            if ex.refresh_token != saved.password {
                self.secrets.save(&Credentials { username: saved.username, password: ex.refresh_token.clone() })?;
            }
        }
        let topics: HashSet<Topic> = changed.iter().filter_map(|(k, _)| topic_of(k)).collect();
        for t in topics {
            self.bus.publish(t);
        }
        Ok(SyncResult { received: changed.len(), sent: ex.sent.len() })
    }

    /// Remember why the last sync didn't work, for the status line.
    pub fn failed(&self, error: &AppError) -> AppResult {
        self.set("problem", Some(&error.to_string()))?;
        if matches!(error, AppError::CloudAuth(_)) {
            self.secrets.clear()?; // the saved sign-in is dead: ask to set up again
        }
        Ok(())
    }

    /// All in one go, on this thread (tests, a one-off "sync now" without a worker).
    pub fn sync(&self) -> AppResult<SyncResult> {
        let job = self.prepare()?;
        match Self::exchange(&*self.cloud, &job) {
            Ok(ex) => self.finish(&ex),
            Err(e) => {
                self.failed(&e)?;
                Err(e)
            }
        }
    }
}

/// A random device id (no rand crate: time and address mixed through the std hasher).
fn rand_u128() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    let s = std::collections::hash_map::RandomState::new();
    let mut h = s.build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let a = h.finish();
    let mut h2 = s.build_hasher();
    h2.write_u64(a ^ 0x9e37_79b9_7f4a_7c15);
    (u128::from(a) << 64) | u128::from(h2.finish())
}
