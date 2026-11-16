//! The Rust planner gives the Python planner's day agendas on the same database (fixtures from tools/quire_fixtures.py).

use std::cell::Cell;
use std::rc::Rc;

use chrono::{NaiveDate, NaiveDateTime};
use quire::application::bus::ChangeBus;
use quire::application::planner::{ItemKind, PlannerService};
use quire::application::ports::Clock;
use quire::infrastructure::repositories::*;
use quire::infrastructure::sqlite::Database;
use serde_json::Value;

struct Fixed(Cell<NaiveDateTime>);
impl Clock for Fixed {
    fn now(&self) -> NaiveDateTime {
        self.0.get()
    }
}

fn kind(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Class => "class",
        ItemKind::Event => "event",
        ItemKind::Routine => "routine",
        ItemKind::Shift => "shift",
        ItemKind::WeeklyShift => "weekly_shift",
    }
}

#[test]
fn day_agendas_match_python() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/agenda.json")).unwrap();
    let dir = std::env::temp_dir().join(format!("quire-agenda-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("python.db");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/python.db"), &path).unwrap();
    let db = Rc::new(Database::open(&path).unwrap());
    let today: NaiveDate = fixture["today"].as_str().unwrap().parse().unwrap();
    let planner = PlannerService {
        courses: Rc::new(SqliteCourses(db.clone())), events: Rc::new(SqliteEvents(db.clone())),
        tasks: Rc::new(SqliteTasks(db.clone())), journal: Rc::new(SqliteJournal(db.clone())),
        clock: Rc::new(Fixed(Cell::new(today.and_hms_opt(12, 0, 0).unwrap()))), bus: Rc::new(ChangeBus::default()),
        jobs: Rc::new(SqliteJobs(db.clone())), shifts: Rc::new(SqliteShifts(db.clone())), routines: Rc::new(SqliteRoutines(db)),
    };
    for expected in fixture["days"].as_array().unwrap() {
        let day: NaiveDate = expected["day"].as_str().unwrap().parse().unwrap();
        let a = planner.day_agenda(day).unwrap();
        let items: Vec<Value> = a.items.iter().map(|i| serde_json::json!([kind(i.kind), i.title, i.time.start, i.time.end, i.room, i.details])).collect();
        assert_eq!(Value::from(items), expected["items"], "items on {day}");
        let tasks: Vec<Value> = a.tasks.iter().map(|t| serde_json::json!([t.task.title, t.overdue])).collect();
        assert_eq!(Value::from(tasks), expected["tasks"], "tasks on {day}");
        assert_eq!(a.journal, expected["journal"].as_str().unwrap(), "journal on {day}");
        assert_eq!(a.is_today, expected["is_today"].as_bool().unwrap());
    }
}
