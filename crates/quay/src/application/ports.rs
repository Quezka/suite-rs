//! What the application needs from the outside world.

use super::QuayError;
use crate::domain::catalog::SuiteApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub page_url: String,
    pub assets: Vec<ReleaseAsset>,
}

/// Where releases are published. Network only: called off the UI thread.
pub trait ReleaseFeed: Send + Sync {
    /// The newest published release, or None when there is none (or the repository is private).
    fn latest(&self, repo: &str) -> Result<Option<Release>, QuayError>;
    /// Fetch the file, check its size and checksum, and return its local path.
    fn download(&self, asset: &ReleaseAsset, progress: &dyn Fn(u64, u64)) -> Result<String, QuayError>;
}

/// How this computer installs, finds and starts apps.
pub trait Platform: Send + Sync {
    fn installed_version(&self, app: &SuiteApp) -> Option<String>;
    /// The installer in a release that fits this computer.
    fn pick<'a>(&self, assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset>;
    fn install(&self, app: &SuiteApp, path: &str) -> Result<(), QuayError>;
    fn launch(&self, app: &SuiteApp) -> Result<(), QuayError>;
}

/// Starting Quay when the user logs in.
pub trait Autostart: Send + Sync {
    fn enabled(&self) -> bool;
    fn set(&self, enabled: bool) -> Result<(), QuayError>;
}
