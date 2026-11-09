//! The launcher's use cases: what is installed, what is new, install or update one app.

use std::collections::HashMap;
use std::sync::Arc;

use super::ports::{Autostart, Platform, Release, ReleaseFeed};
use super::QuayError;
use crate::domain::catalog::{by_key, SuiteApp, CATALOG};
use crate::domain::versions::is_newer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    NotInstalled,
    UpToDate,
    Update,
    /// Nothing to install yet (no release for this computer).
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStatus {
    pub app: SuiteApp,
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub state: State,
    pub page_url: String,
}

pub type Releases = HashMap<&'static str, Option<Release>>;

pub struct SuiteService {
    feed: Arc<dyn ReleaseFeed>,
    platform: Arc<dyn Platform>,
    autostart: Option<Arc<dyn Autostart>>,
    catalog: Vec<SuiteApp>,
    releases: Releases,
}

impl SuiteService {
    pub fn new(feed: Arc<dyn ReleaseFeed>, platform: Arc<dyn Platform>, autostart: Option<Arc<dyn Autostart>>) -> Self {
        Self { feed, platform, autostart, catalog: CATALOG.to_vec(), releases: HashMap::new() }
    }

    pub fn feed(&self) -> Arc<dyn ReleaseFeed> {
        self.feed.clone()
    }

    pub fn platform(&self) -> Arc<dyn Platform> {
        self.platform.clone()
    }

    pub fn apps(&self) -> &[SuiteApp] {
        &self.catalog
    }

    /// Ask the feed about every app. Network only: run it off the UI thread with the `feed()` it returns.
    pub fn fetch(feed: &dyn ReleaseFeed, apps: &[SuiteApp]) -> Result<Releases, QuayError> {
        let mut found = HashMap::new();
        for app in apps {
            found.insert(app.key, feed.latest(app.repo)?);
        }
        Ok(found)
    }

    pub fn remember(&mut self, releases: Releases) {
        self.releases = releases;
    }

    pub fn statuses(&self) -> Vec<AppStatus> {
        self.catalog
            .iter()
            .map(|app| {
                let installed = self.platform.installed_version(app);
                let release = self.releases.get(app.key).and_then(Option::as_ref);
                let usable = release.is_some_and(|r| self.platform.pick(&r.assets).is_some());
                let latest = release.map(|r| r.version.clone());
                let state = match &installed {
                    None if usable => State::NotInstalled,
                    None => State::Unavailable,
                    Some(v) if usable && is_newer(latest.as_deref(), Some(v)) => State::Update,
                    Some(_) => State::UpToDate,
                };
                AppStatus { app: *app, installed, latest, state, page_url: release.map(|r| r.page_url.clone()).unwrap_or_default() }
            })
            .collect()
    }

    /// The installer to download for an app, if there is one for this computer.
    pub fn asset_for(&self, key: &str) -> Result<super::ports::ReleaseAsset, QuayError> {
        self.releases
            .get(key)
            .and_then(Option::as_ref)
            .and_then(|r| self.platform.pick(&r.assets).cloned())
            .ok_or_else(|| QuayError::new("There is no installer for this app on this computer yet."))
    }

    pub fn launch(&self, key: &str) -> Result<(), QuayError> {
        let app = by_key(key).ok_or_else(|| QuayError::new("Unknown app."))?;
        self.platform.launch(app)
    }

    pub fn autostart_supported(&self) -> bool {
        self.autostart.is_some()
    }

    pub fn autostart_enabled(&self) -> bool {
        self.autostart.as_ref().is_some_and(|a| a.enabled())
    }

    pub fn set_autostart(&self, enabled: bool) -> Result<(), QuayError> {
        match &self.autostart {
            Some(a) => a.set(enabled),
            None => Ok(()),
        }
    }

    /// The apps with something new, Quay itself last: updating it may close it.
    pub fn pending_updates(&self) -> Vec<&'static str> {
        let mut keys: Vec<_> = self.statuses().into_iter().filter(|s| s.state == State::Update).map(|s| s.app.key).collect();
        keys.sort_by_key(|k| *k == "quay");
        keys
    }

    pub fn has_updates(&self) -> bool {
        self.statuses().iter().any(|s| s.state == State::Update)
    }
}

/// Download and install one app: network, then the system's installer. Runs off the UI thread.
pub fn download_and_install(feed: &dyn ReleaseFeed, platform: &dyn Platform, key: &str, asset: &super::ports::ReleaseAsset,
                            progress: &dyn Fn(u64, u64), installing: &dyn Fn()) -> Result<(), QuayError> {
    let app = by_key(key).ok_or_else(|| QuayError::new("Unknown app."))?;
    let path = feed.download(asset, progress)?;
    installing();
    platform.install(app, &path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::demo::{DemoFeed, DemoPlatform};

    fn service() -> SuiteService {
        let feed: Arc<dyn ReleaseFeed> = Arc::new(DemoFeed::default());
        let mut s = SuiteService::new(feed.clone(), Arc::new(DemoPlatform::default()), None);
        let releases = SuiteService::fetch(&*feed, s.apps()).unwrap();
        s.remember(releases);
        s
    }

    #[test]
    fn states_follow_what_is_installed_and_released() {
        let s = service();
        let by = |k: &str| s.statuses().into_iter().find(|x| x.app.key == k).unwrap().state;
        assert_eq!(by("quire"), State::Update);
        assert_eq!(by("gantry"), State::UpToDate);
        assert_eq!(by("folio"), State::NotInstalled);
        assert_eq!(by("missive"), State::Unavailable);
    }

    #[test]
    fn quay_updates_last() {
        let s = service();
        let pending = s.pending_updates();
        assert!(pending.contains(&"quire"));
        assert_eq!(pending.last(), Some(&"quay"));
    }

    #[test]
    fn installing_moves_an_app_up_to_date() {
        let s = service();
        let asset = s.asset_for("quire").unwrap();
        download_and_install(&*s.feed(), &*s.platform(), "quire", &asset, &|_, _| {}, &|| {}).unwrap();
        assert!(!s.pending_updates().contains(&"quire"));
    }
}
