//! updates: use case w/ fakes, github feed w/ canned answers (= python `tests/test_updates.py`); never network

use std::cell::Cell;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use chrono::{Duration, NaiveDate, NaiveDateTime};
use quire::application::errors::{AppError, AppResult};
use quire::application::ports::{Clock, ReleaseAsset, ReleaseFeed, ReleaseInfo, UpdateInstaller};
use quire::application::updates::{parse_version, UpdateService};
use quire::infrastructure::repositories::SqliteSettings;
use quire::infrastructure::sqlite::Database;
use quire::infrastructure::updates::{deb_architecture, DebInstaller, Fetch, GitHubReleaseFeed, NoInstaller};
use sha2::{Digest, Sha256};

struct Fixed(Cell<NaiveDateTime>);
impl Clock for Fixed {
    fn now(&self) -> NaiveDateTime {
        self.0.get()
    }
}

fn deb() -> ReleaseAsset {
    ReleaseAsset { name: format!("quire_9.0.0_{}.deb", deb_architecture()), url: "https://x/quire.deb".into(), size: 10, sha256: None }
}

fn release(version: &str) -> ReleaseInfo {
    ReleaseInfo { version: version.into(), notes: "## New\n- Things".into(), page_url: "https://github.com/Quezka/quire/releases".into(),
        assets: vec![deb(), ReleaseAsset { name: "Quire-9.0.0-windows-x64-setup.exe".into(), url: "https://x/setup.exe".into(), size: 10, sha256: None }] }
}

#[derive(Default)]
struct FakeReleases(Mutex<Option<ReleaseInfo>>);
impl ReleaseFeed for FakeReleases {
    fn latest(&self) -> AppResult<Option<ReleaseInfo>> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn download(&self, asset: &ReleaseAsset, progress: &mut dyn FnMut(u64, u64)) -> AppResult<PathBuf> {
        progress(asset.size, asset.size);
        Ok(PathBuf::from(format!("/tmp/{}", asset.name)))
    }
}

struct FakeInstaller {
    supported: bool,
    installed: Mutex<Vec<PathBuf>>,
}
impl UpdateInstaller for FakeInstaller {
    fn supported(&self) -> bool {
        self.supported
    }
    fn pick(&self, assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
        DebInstaller { executable: PathBuf::new() }.pick(assets)
    }
    fn install(&self, path: &Path) -> AppResult<bool> {
        self.installed.lock().unwrap().push(path.into());
        Ok(false)
    }
}

fn service(supported: bool) -> (UpdateService, Arc<FakeReleases>, Arc<FakeInstaller>, Rc<Fixed>) {
    let db = Rc::new(Database::in_memory().unwrap());
    let feed = Arc::new(FakeReleases::default());
    let installer = Arc::new(FakeInstaller { supported, installed: Mutex::default() });
    let clock = Rc::new(Fixed(Cell::new(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap().and_hms_opt(9, 0, 0).unwrap())));
    let s = UpdateService { feed: feed.clone(), installer: installer.clone(), settings: Rc::new(SqliteSettings(db)),
        clock: clock.clone(), current_version: "0.51.0".into() };
    (s, feed, installer, clock)
}

#[test]
fn versions_compare_as_numbers() {
    assert!(parse_version("v0.10.0") > parse_version("0.9.9"));
    assert!(parse_version("1.0.0") > parse_version("0.99.0"));
    assert_eq!(parse_version("0.11.0"), vec![0, 11, 0]);
    assert!(parse_version("nonsense") < parse_version("0.0.1"));
    assert_eq!(parse_version("1.2.3-rc1"), vec![1, 2, 3]);
}

#[test]
fn newer_release_offered_with_right_file_then_downloaded_and_installed() {
    let (s, feed, installer, _) = service(true);
    let w = s.worker();
    assert_eq!(w.check().unwrap(), None);
    *feed.0.lock().unwrap() = Some(release("0.51.0"));
    assert_eq!(w.check().unwrap(), None);
    *feed.0.lock().unwrap() = Some(release("0.0.1"));
    assert_eq!(w.check().unwrap(), None);
    *feed.0.lock().unwrap() = Some(release("9.0.0"));
    let u = w.check().unwrap().unwrap();
    assert_eq!((u.version.as_str(), u.asset.clone(), u.can_install, u.current.as_str()), ("9.0.0", Some(deb()), true, "0.51.0"));
    let mut seen = vec![];
    let path = w.download(&u, &mut |d, t| seen.push((d, t))).unwrap();
    assert_eq!(seen.last(), Some(&(10, 10)));
    assert!(!w.install(&path).unwrap());
    assert_eq!(*installer.installed.lock().unwrap(), vec![path]);
}

#[test]
fn copy_that_cant_install_gets_download_page() {
    let (s, feed, _, _) = service(false);
    *feed.0.lock().unwrap() = Some(release("9.0.0"));
    let u = s.worker().check().unwrap().unwrap();
    assert!(!u.can_install && u.page_url.ends_with("/releases"));
}

#[test]
fn daily_check_skip_and_switch_off() {
    let (s, _, _, clock) = service(true);
    assert!(s.due());
    s.mark_checked().unwrap();
    assert!(!s.due());
    clock.0.set(clock.0.get() + Duration::hours(21));
    assert!(s.due());
    s.set_auto_check(false).unwrap();
    assert!(!s.due() && !s.auto_check());
    s.skip("9.0.0").unwrap();
    assert!(s.skipped("9.0.0") && !s.skipped("9.0.1"));
}

// ---- github feed

enum Answer {
    Status(u16, Vec<u8>),
    Offline,
}

struct Canned {
    answers: Mutex<Vec<Answer>>,
    seen: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>,
}
impl Canned {
    fn new(answers: Vec<Answer>) -> Box<Self> {
        Box::new(Self { answers: Mutex::new(answers), seen: Arc::default() })
    }
}
impl Fetch for Canned {
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<(u16, Box<dyn Read + Send>), String> {
        self.seen.lock().unwrap().push((url.into(), headers.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()));
        match self.answers.lock().unwrap().remove(0) {
            Answer::Status(code, body) => Ok((code, Box::new(std::io::Cursor::new(body)))),
            Answer::Offline => Err("offline".into()),
        }
    }
}

fn sha(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn feed(answers: Vec<Answer>, folder: Option<PathBuf>) -> GitHubReleaseFeed {
    GitHubReleaseFeed { repo: "Quezka/quire".into(), user_agent: "Quire/0.11.0".into(), fetch: Canned::new(answers), folder }
}

fn update_err(r: AppResult<impl std::fmt::Debug>) -> String {
    match r {
        Err(AppError::Update(m)) => m,
        other => panic!("expected update error, got {other:?}"),
    }
}

#[test]
fn latest_release_is_read_from_github() {
    let body = serde_json::json!({
        "tag_name": "v0.12.0", "body": "Notes", "html_url": "https://github.com/Quezka/quire/releases/tag/v0.12.0",
        "draft": false, "prerelease": false,
        "assets": [{"name": "quire_0.12.0_amd64.deb", "browser_download_url": "https://dl/deb", "size": 5, "digest": format!("sha256:{}", sha(b"hello"))},
                   {"name": "Quire-0.12.0-windows-x64-setup.exe", "browser_download_url": "https://dl/exe", "size": 7}]
    });
    let canned = Canned::new(vec![Answer::Status(200, body.to_string().into_bytes())]);
    let seen = canned.seen.clone();
    let f = GitHubReleaseFeed { repo: "Quezka/quire".into(), user_agent: "Quire/0.11.0".into(), fetch: canned, folder: None };
    let info = f.latest().unwrap().unwrap();
    assert_eq!((info.version.as_str(), info.notes.as_str()), ("0.12.0", "Notes"));
    assert_eq!(info.assets[0].sha256.as_deref(), Some(sha(b"hello").as_str()));
    assert_eq!(info.assets[1].sha256, None);
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].0, "https://api.github.com/repos/Quezka/quire/releases/latest");
    assert!(seen[0].1.contains(&("User-Agent".into(), "Quire/0.11.0".into())));
}

#[test]
fn no_release_rate_limits_and_offline() {
    assert_eq!(feed(vec![Answer::Status(404, b"{}".to_vec())], None).latest().unwrap(), None);
    let draft = serde_json::json!({"tag_name": "v1", "draft": true}).to_string().into_bytes();
    assert_eq!(feed(vec![Answer::Status(200, draft)], None).latest().unwrap(), None);
    assert!(update_err(feed(vec![Answer::Status(403, b"{}".to_vec())], None).latest()).contains("limiting"));
    assert!(update_err(feed(vec![Answer::Offline], None).latest()).contains("internet"));
    assert!(update_err(feed(vec![Answer::Status(200, b"<html>".to_vec())], None).latest()).contains("understand"));
}

#[test]
fn download_checks_size_and_checksum() {
    let dir = std::env::temp_dir().join(format!("quire-upd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let good = ReleaseAsset { name: "quire.deb".into(), url: "https://dl/deb".into(), size: 5, sha256: Some(sha(b"hello")) };
    let mut seen = vec![];
    let path = feed(vec![Answer::Status(200, b"hello".to_vec())], Some(dir.clone())).download(&good, &mut |d, t| seen.push((d, t))).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    assert_eq!(seen.last(), Some(&(5, 5)));
    assert!(update_err(feed(vec![Answer::Status(200, b"hellO".to_vec())], Some(dir.clone())).download(&good, &mut |_, _| {})).contains("damaged"));
    let unsigned = ReleaseAsset { sha256: None, ..good.clone() };
    assert!(update_err(feed(vec![Answer::Status(200, b"hel".to_vec())], Some(dir.clone())).download(&unsigned, &mut |_, _| {})).contains("damaged"));
    assert!(update_err(feed(vec![Answer::Offline], Some(dir.clone())).download(&good, &mut |_, _| {})).contains("didn't finish"));
    let parts = std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "part")).count();
    assert_eq!(parts, 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn installers_pick_their_file() {
    let assets = release("9.0.0").assets;
    assert_eq!(DebInstaller { executable: PathBuf::new() }.pick(&assets), Some(deb()));
    assert!(!DebInstaller { executable: PathBuf::from("/home/x/quire") }.supported());
    assert!(!NoInstaller.supported() && NoInstaller.pick(&assets).is_none());
    assert!(NoInstaller.install(Path::new("x")).is_err());
}
