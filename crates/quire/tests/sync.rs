//! Two devices syncing through a cloud (a fake one in memory), as the Python app's tests/test_sync.py.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, NaiveDateTime};
use quire::application::bus::ChangeBus;
use quire::application::errors::{AppError, AppResult};
use quire::application::ideas::IdeaService;
use quire::application::ports::*;
use quire::application::sync::SyncService;
use quire::application::tasks::{TaskInput, TaskService};
use quire::infrastructure::repositories::*;
use quire::infrastructure::sqlite::Database;

struct Fixed(Cell<NaiveDateTime>);
impl Clock for Fixed {
    fn now(&self) -> NaiveDateTime {
        self.0.get()
    }
}

#[derive(Default)]
struct FakeCloud {
    records: Mutex<Vec<SyncRecord>>, // in arrival order: the cursor is an index
}

impl CloudBackend for FakeCloud {
    fn sign_up(&self, _c: &CloudConfig, email: &str, _p: &str, _i: &str) -> AppResult<CloudSession> {
        Ok(CloudSession { user_id: "u".into(), email: email.into(), token: "t".into(), refresh_token: "r1".into() })
    }
    fn sign_in(&self, c: &CloudConfig, email: &str, p: &str) -> AppResult<CloudSession> {
        if p != "secret" {
            return Err(AppError::CloudAuth("Wrong password.".into()));
        }
        self.sign_up(c, email, p, "")
    }
    fn refresh(&self, _c: &CloudConfig, token: &str) -> AppResult<CloudSession> {
        Ok(CloudSession { user_id: "u".into(), email: "me@x.it".into(), token: "t".into(), refresh_token: token.into() })
    }
    fn pull(&self, _c: &CloudConfig, _s: &CloudSession, cursor: Option<&str>) -> AppResult<(Vec<SyncRecord>, Option<String>)> {
        let all = self.records.lock().unwrap();
        let from: usize = cursor.and_then(|c| c.parse().ok()).unwrap_or(0);
        Ok((all[from..].to_vec(), Some(all.len().to_string())))
    }
    fn push(&self, _c: &CloudConfig, _s: &CloudSession, records: &[SyncRecord]) -> AppResult {
        self.records.lock().unwrap().extend_from_slice(records);
        Ok(())
    }
}

#[derive(Default)]
struct Keyring(RefCell<Option<Credentials>>);
impl CredentialStore for Keyring {
    fn load(&self) -> Option<Credentials> {
        self.0.borrow().clone()
    }
    fn save(&self, c: &Credentials) -> AppResult {
        *self.0.borrow_mut() = Some(c.clone());
        Ok(())
    }
    fn clear(&self) -> AppResult {
        *self.0.borrow_mut() = None;
        Ok(())
    }
}

struct Device {
    tasks: Rc<TaskService>,
    ideas: IdeaService,
    sync: SyncService,
}

fn device(cloud: Arc<FakeCloud>) -> Device {
    let db = Rc::new(Database::in_memory().unwrap());
    let clock: Rc<dyn Clock> = Rc::new(Fixed(Cell::new(NaiveDate::from_ymd_opt(2026, 9, 23).unwrap().and_hms_opt(12, 0, 0).unwrap())));
    let bus = Rc::new(ChangeBus::default());
    let settings: Rc<dyn KeyValueStore> = Rc::new(SqliteSettings(db.clone()));
    let tasks = Rc::new(TaskService::new(Rc::new(SqliteTasks(db.clone())), Rc::new(SqliteCourses(db.clone())), clock.clone(), bus.clone()));
    let ideas = IdeaService::new(Rc::new(SqliteIdeas(db.clone())), clock.clone(), bus.clone(), tasks.clone(), settings.clone());
    let sync = SyncService { store: Rc::new(SqliteSync(db)), cloud, secrets: Rc::new(Keyring::default()), settings, clock, bus };
    Device { tasks, ideas, sync }
}

#[test]
fn two_devices_share_tasks_and_ideas_and_the_newer_change_wins() {
    let cloud = Arc::new(FakeCloud::default());
    let (a, b) = (device(cloud.clone()), device(cloud.clone()));
    let config = SyncService::check("quire-sync-1a2b3", "AIzaSyApgdK73ulUW8XjbcoiQThuTpmL30GiIXg", "me@x.it", "secret").unwrap();
    assert!(SyncService::check("Bad Project", "k", "x", "").is_err());
    for d in [&a, &b] {
        let session = SyncService::authenticate(&*cloud, &config, "me@x.it", "secret", false, "").unwrap();
        d.sync.connect(&config, &session, false).unwrap();
    }
    assert!(SyncService::authenticate(&*cloud, &config, "me@x.it", "nope", false, "").is_err());
    let t = a.tasks.save(None, &TaskInput::titled("Lab report")).unwrap();
    a.ideas.add("A shelf for the games #home").unwrap();
    let sent = a.sync.sync().unwrap();
    assert!(sent.sent >= 2);
    let got = b.sync.sync().unwrap();
    assert_eq!(got.received, sent.sent);
    assert_eq!(b.tasks.groups(true, None).unwrap()[0].items[0].task.title, "Lab report");
    assert_eq!(b.ideas.ideas(true, "#home").unwrap().len(), 1);
    assert_eq!(a.sync.status().unwrap().pending, 0);
    // a change on A reaches B
    a.tasks.set_done(t, true).unwrap();
    a.sync.sync().unwrap();
    b.sync.sync().unwrap();
    assert!(b.tasks.groups(true, None).unwrap().iter().flat_map(|g| g.items.iter()).all(|i| i.task.done));
    let link = a.sync.phone_link(None).unwrap();
    assert!(link.starts_with("quire-link:"));
    let server = SyncService::check_server("sync.example.com/quire/", "me@x.it", "pw").unwrap();
    assert_eq!(server.project_id, "https://sync.example.com/quire");
}
