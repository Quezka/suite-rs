//! updating quire from inside app (port of `services/updates.py`)
//! `check` + `download` = network, `install` may block => all on worker thread

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use chrono::{Duration, NaiveDateTime};

use super::errors::{AppError, AppResult};
use super::ports::{Clock, KeyValueStore, ReleaseAsset, ReleaseFeed, UpdateInstaller};

pub const CHECK_EVERY_HOURS: i64 = 20;
const PREFIX: &str = "updates.";

/// `v0.11.0` -> `[0, 11, 0]`; unparseable -> `[0]` = oldest
pub fn parse_version(text: &str) -> Vec<u64> {
    let head = text.split('-').next().unwrap_or("");
    let mut out = Vec::new();
    let mut digits = String::new();
    for ch in head.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            out.push(digits.parse().unwrap_or(u64::MAX));
            digits.clear();
        }
    }
    out.truncate(3);
    if out.is_empty() {
        out.push(0);
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub version: String,
    pub current: String,
    pub notes: String,
    pub page_url: String,
    /// file for this platform
    pub asset: Option<ReleaseAsset>,
    /// false -> send user to download page
    pub can_install: bool,
}

pub struct UpdateService {
    pub feed: Arc<dyn ReleaseFeed>,
    pub installer: Arc<dyn UpdateInstaller>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub current_version: String,
}

/// network half, `Send` => move to worker thread
#[derive(Clone)]
pub struct UpdateWorker {
    pub feed: Arc<dyn ReleaseFeed>,
    pub installer: Arc<dyn UpdateInstaller>,
    pub current_version: String,
}

impl UpdateService {
    fn key(name: &str) -> String {
        format!("{PREFIX}{name}")
    }

    pub fn auto_check(&self) -> bool {
        self.settings.get(&Self::key("auto")).as_deref() != Some("0")
    }

    pub fn set_auto_check(&self, on: bool) -> AppResult {
        self.settings.set(&Self::key("auto"), Some(if on { "1" } else { "0" }))
    }

    /// time for auto check (~1/day)
    pub fn due(&self) -> bool {
        if !self.auto_check() {
            return false;
        }
        match self.settings.get(&Self::key("last_check")).and_then(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S").ok()) {
            Some(last) => self.clock.now() - last >= Duration::hours(CHECK_EVERY_HOURS),
            None => true,
        }
    }

    pub fn mark_checked(&self) -> AppResult {
        self.settings.set(&Self::key("last_check"), Some(&self.clock.now().format("%Y-%m-%dT%H:%M:%S").to_string()))
    }

    pub fn skip(&self, version: &str) -> AppResult {
        self.settings.set(&Self::key("skipped"), Some(version))
    }

    pub fn skipped(&self, version: &str) -> bool {
        self.settings.get(&Self::key("skipped")).as_deref() == Some(version)
    }

    pub fn worker(&self) -> UpdateWorker {
        UpdateWorker { feed: self.feed.clone(), installer: self.installer.clone(), current_version: self.current_version.clone() }
    }
}

impl UpdateWorker {
    /// newest release if newer than this copy
    pub fn check(&self) -> AppResult<Option<AvailableUpdate>> {
        let Some(release) = self.feed.latest()? else { return Ok(None) };
        if parse_version(&release.version) <= parse_version(&self.current_version) {
            return Ok(None);
        }
        let asset = self.installer.pick(&release.assets);
        let can_install = asset.is_some() && self.installer.supported();
        Ok(Some(AvailableUpdate {
            version: release.version, current: self.current_version.clone(), notes: release.notes,
            page_url: release.page_url, asset, can_install,
        }))
    }

    pub fn download(&self, update: &AvailableUpdate, progress: &mut dyn FnMut(u64, u64)) -> AppResult<PathBuf> {
        let asset = update.asset.as_ref()
            .ok_or_else(|| AppError::Update("There's no download for this computer in that release.".into()))?;
        self.feed.download(asset, progress)
    }

    pub fn install(&self, path: &Path) -> AppResult<bool> {
        self.installer.install(path)
    }
}
