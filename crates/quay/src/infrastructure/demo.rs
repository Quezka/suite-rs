//! Made-up releases and installs for `quay --demo` and the tests: nothing is downloaded or installed.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::application::ports::{Platform, Release, ReleaseAsset, ReleaseFeed};
use crate::application::QuayError;
use crate::domain::catalog::SuiteApp;

const LATEST: [(&str, &str); 8] = [("quire", "0.46.1"), ("gantry", "0.13.1"), ("ligature", "0.17.1"), ("folio", "0.9.1"),
    ("bivouac", "0.13.1"), ("facet", "0.7.1"), ("natter", "0.9.2"), ("quay", "0.2.0")]; // missive: none, to show how that looks
const INSTALLED: [(&str, &str); 5] = [("quire", "0.45.0"), ("gantry", "0.13.1"), ("ligature", "0.17.0"), ("natter", "0.9.2"), ("quay", "0.1.0")];

#[derive(Default)]
pub struct DemoFeed;

impl ReleaseFeed for DemoFeed {
    fn latest(&self, repo: &str) -> Result<Option<Release>, QuayError> {
        let key = repo.rsplit('/').next().unwrap_or(repo);
        Ok(LATEST.iter().find(|(k, _)| *k == key).map(|(k, v)| {
            let deb = format!("{k}_{v}_amd64.deb");
            Release {
                version: v.to_string(),
                page_url: format!("https://github.com/{repo}/releases/tag/v{v}"),
                assets: vec![
                    ReleaseAsset { name: deb.clone(), url: format!("demo://{deb}"), size: 1000, sha256: None },
                    ReleaseAsset { name: format!("{}-{v}-windows-x64-setup.exe", k), url: "demo://setup".into(), size: 1000, sha256: None },
                ],
            }
        }))
    }

    fn download(&self, asset: &ReleaseAsset, progress: &dyn Fn(u64, u64)) -> Result<String, QuayError> {
        for done in (0..=asset.size).step_by(250) {
            progress(done, asset.size);
            if !cfg!(test) {
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
        }
        Ok(format!("/nonexistent/{}", asset.name))
    }
}

pub struct DemoPlatform {
    installed: Mutex<HashMap<&'static str, String>>,
}

impl Default for DemoPlatform {
    fn default() -> Self {
        Self { installed: Mutex::new(INSTALLED.iter().map(|(k, v)| (*k, v.to_string())).collect()) }
    }
}

impl Platform for DemoPlatform {
    fn installed_version(&self, app: &SuiteApp) -> Option<String> {
        self.installed.lock().ok()?.get(app.key).cloned()
    }
    fn pick<'a>(&self, assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset> {
        assets.first()
    }
    fn install(&self, app: &SuiteApp, _path: &str) -> Result<(), QuayError> {
        let version = LATEST.iter().find(|(k, _)| *k == app.key).map(|(_, v)| v.to_string());
        if let (Ok(mut map), Some(v)) = (self.installed.lock(), version) {
            map.insert(app.key, v);
        }
        Ok(())
    }
    fn launch(&self, _app: &SuiteApp) -> Result<(), QuayError> {
        Ok(())
    }
}
