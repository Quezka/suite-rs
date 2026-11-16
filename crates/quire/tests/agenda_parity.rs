//! The Rust planner gives the Python planner's day agendas on the same database (fixtures from tools/quire_fixtures.py).

use std::cell::Cell;
use std::rc::Rc;

use chrono::{NaiveDate, NaiveDateTime};
use quire::application::bus::ChangeBus;
use quire::application::digest::DigestService;
use quire::application::ideas::IdeaService;
use quire::application::notes::{class_note_title, NoteService};
use quire::application::tasks::TaskService;
use quire::application::work::WorkService;
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
    let clock: Rc<dyn Clock> = Rc::new(Fixed(Cell::new(today.and_hms_opt(12, 0, 0).unwrap())));
    let bus = Rc::new(ChangeBus::default());
    let planner = PlannerService {
        courses: Rc::new(SqliteCourses(db.clone())), events: Rc::new(SqliteEvents(db.clone())),
        tasks: Rc::new(SqliteTasks(db.clone())), journal: Rc::new(SqliteJournal(db.clone())),
        clock: clock.clone(), bus: bus.clone(),
        jobs: Rc::new(SqliteJobs(db.clone())), shifts: Rc::new(SqliteShifts(db.clone())), routines: Rc::new(SqliteRoutines(db.clone())),
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
    let tasks = Rc::new(TaskService::new(Rc::new(SqliteTasks(db.clone())), Rc::new(SqliteCourses(db.clone())), clock.clone(), bus.clone()));
    let settings = Rc::new(SqliteSettings(db.clone()));
    let ideas = Rc::new(IdeaService::new(Rc::new(SqliteIdeas(db.clone())), clock.clone(), bus, tasks.clone(), settings.clone()));
    let notes = NoteService {
        notes: Rc::new(SqliteNotes(db.clone())), courses: Rc::new(SqliteCourses(db.clone())), lookup: Rc::new(SqliteCourses(db.clone())),
        clock: clock.clone(), bus: Rc::new(ChangeBus::default()), images: Rc::new(SqliteImages(db.clone())),
        notebooks: Rc::new(SqliteNotebooks(db.clone())),
    };
    let found: Vec<Value> = notes.search("", None, None).unwrap().iter().map(|n| serde_json::json!([n.title, n.pinned, n.topic, n.snippet,
        n.course.as_ref().map(|c| c.name.clone()), n.notebook.as_ref().map(|b| b.name.clone())])).collect();
    assert_eq!(Value::from(found), fixture["notes"]);
    let groups: Vec<Value> = notes.grouped("", None, None).unwrap().iter().map(|g| serde_json::json!([g.course.as_ref().map(|c| c.name.clone()),
        g.notebook.as_ref().map(|b| b.name.clone()), g.topic, g.notes.len()])).collect();
    assert_eq!(Value::from(groups), fixture["note_groups"]);
    assert_eq!(class_note_title("Biology", "2026-11-16".parse().unwrap()), fixture["class_note_title"].as_str().unwrap());
    let work = WorkService { jobs: Rc::new(SqliteJobs(db.clone())), shifts: Rc::new(SqliteShifts(db.clone())), clock: clock.clone(),
                             bus: Rc::new(ChangeBus::default()) };
    for expected in fixture["work"].as_array().unwrap() {
        let first: NaiveDate = expected["first"].as_str().unwrap().parse().unwrap();
        let last: NaiveDate = expected["last"].as_str().unwrap().parse().unwrap();
        let w = work.summary(first, last).unwrap();
        assert_eq!(serde_json::json!([w.minutes, w.pay, w.net, w.shift_count]),
                   serde_json::json!([expected["minutes"], expected["pay"], expected["net"], expected["shifts"]]), "work {first}");
        let per: Vec<Value> = w.per_job.iter().map(|t| serde_json::json!([t.job.as_ref().map(|j| j.name.clone()), t.minutes, t.pay, t.net])).collect();
        assert_eq!(Value::from(per), expected["per_job"]);
        let items: Vec<Value> = work.shifts_between(first, last).unwrap().iter()
            .map(|i| serde_json::json!([i.shift.day.to_string(), i.shift.start, i.shift.duration, i.pay, i.net])).collect();
        assert_eq!(Value::from(items), expected["items"]);
    }
    let digest = DigestService { planner: Rc::new(planner), tasks, ideas, settings, clock };
    for expected in fixture["digests"].as_array().unwrap() {
        let morning = expected["morning"].as_bool().unwrap();
        let g = digest.build(morning, today, false).unwrap();
        let items: Vec<Value> = g.items.iter().map(|(m, t)| serde_json::json!([m, t])).collect();
        assert_eq!(Value::from(items), expected["items"], "digest items morning={morning}");
        assert_eq!(serde_json::json!(g.due), expected["due"]);
        assert_eq!(g.overdue as u64, expected["overdue"].as_u64().unwrap());
        assert_eq!(serde_json::json!(g.ideas), expected["ideas"]);
        assert_eq!(g.idea_back, expected["idea_back"].as_str().unwrap());
    }
}
