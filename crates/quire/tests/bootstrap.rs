//! wiring smoke: python db -> every service answers; outside world faked (no keyring, calendar, login entry)

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{NaiveDate, NaiveDateTime};
use quire::application::errors::AppResult;
use quire::application::ports::{Clock, CredentialStore, Credentials, LoginItem, SystemCalendar};
use quire::application::tasks::TaskInput;
use quire::bootstrap::{build_services, Overrides};
use quire::infrastructure::quiet::NoQuiet;

struct Fixed(Cell<NaiveDateTime>);
impl Clock for Fixed {
    fn now(&self) -> NaiveDateTime {
        self.0.get()
    }
}

#[derive(Default)]
struct Keyring(RefCell<Option<Credentials>>);
impl CredentialStore for Keyring {
    fn load(&self) -> Option<Credentials> { self.0.borrow().clone() }
    fn save(&self, c: &Credentials) -> AppResult { *self.0.borrow_mut() = Some(c.clone()); Ok(()) }
    fn clear(&self) -> AppResult { *self.0.borrow_mut() = None; Ok(()) }
}

#[derive(Default)]
struct Calendar(RefCell<Vec<String>>);
impl SystemCalendar for Calendar {
    fn can_subscribe(&self) -> bool { true }
    fn publish(&self, text: &str) -> AppResult { self.0.borrow_mut().push(text.into()); Ok(()) }
    fn connect(&self) -> AppResult { Ok(()) }
    fn disconnect(&self) -> AppResult { Ok(()) }
    fn open(&self) -> AppResult { Ok(()) }
}

#[derive(Default)]
struct Login(Cell<bool>);
impl LoginItem for Login {
    fn supported(&self) -> bool { true }
    fn enabled(&self) -> bool { self.0.get() }
    fn set_enabled(&self, on: bool) -> Result<(), String> { self.0.set(on); Ok(()) }
}

#[test]
fn every_service_answers_on_the_python_database() {
    let dir = std::env::temp_dir().join(format!("quire-boot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("python.db");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/python.db"), &path).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
    let calendar = Rc::new(Calendar::default());
    let s = build_services(&path, Overrides {
        clock: Some(Rc::new(Fixed(Cell::new(today.and_hms_opt(12, 0, 0).unwrap())))),
        credentials: Some(Rc::new(Keyring::default())), sync_secrets: Some(Rc::new(Keyring::default())),
        calendar: Some(calendar.clone()), login_item: Some(Rc::new(Login::default())), quiet: Some(Rc::new(NoQuiet)),
        ..Default::default()
    }).unwrap();
    let id = s.tasks.save(None, &TaskInput::titled("Wired")).unwrap();
    assert_eq!(s.tasks.task(id).unwrap().title, "Wired");
    s.planner.day_agenda(today).unwrap();
    s.digest.build(true, today, false).unwrap();
    s.school.overview(None).unwrap();
    s.sync.status().unwrap();
    s.calendar.set_enabled(true).unwrap();
    assert!(calendar.0.borrow()[0].starts_with("BEGIN:VCALENDAR\r\n"));
    s.startup.set_start_on_login(true).unwrap();
    assert!(s.startup.start_on_login());
    assert!(s.updates.due());
    std::fs::remove_dir_all(&dir).unwrap();
}
