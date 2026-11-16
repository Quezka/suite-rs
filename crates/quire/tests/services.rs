//! The use cases on a real (in-memory) database: same behaviour as the Python app's tests (test_ideas.py, tasks).

use std::cell::Cell;
use std::rc::Rc;

use chrono::{NaiveDate, NaiveDateTime};
use quire::application::bus::{ChangeBus, Topic};
use quire::application::ideas::IdeaService;
use quire::application::ports::Clock;
use quire::application::tasks::{TaskInput, TaskService};
use quire::domain::model::DueBucket;
use quire::infrastructure::repositories::{SqliteCourses, SqliteIdeas, SqliteSettings, SqliteTasks};
use quire::infrastructure::sqlite::Database;

struct Fixed(Cell<NaiveDateTime>);
impl Clock for Fixed {
    fn now(&self) -> NaiveDateTime {
        self.0.get()
    }
}

fn day(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn setup() -> (Rc<TaskService>, IdeaService, Rc<Fixed>, Rc<ChangeBus>) {
    let db = Rc::new(Database::in_memory().unwrap());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(12, 0, 0).unwrap())));
    let bus = Rc::new(ChangeBus::default());
    let tasks = Rc::new(TaskService::new(Rc::new(SqliteTasks(db.clone())), Rc::new(SqliteCourses(db.clone())), clock.clone(), bus.clone()));
    let ideas = IdeaService::new(Rc::new(SqliteIdeas(db.clone())), clock.clone(), bus.clone(), tasks.clone(), Rc::new(SqliteSettings(db)));
    (tasks, ideas, clock, bus)
}

#[test]
fn tasks_group_by_when_they_are_due_and_one_comes_next() {
    let (tasks, _, _, bus) = setup();
    let heard = Rc::new(Cell::new(0));
    let h = heard.clone();
    bus.subscribe(move |t| if t == Topic::Tasks { h.set(h.get() + 1) });
    let mut late = TaskInput::titled("Lab report");
    late.due = Some(day("2026-09-21"));
    let mut today = TaskInput::titled("Buy a calculator");
    today.due = Some(day("2026-09-23"));
    tasks.save(None, &TaskInput::titled("Someday")).unwrap();
    let a = tasks.save(None, &late).unwrap();
    tasks.save(None, &today).unwrap();
    let groups = tasks.groups(false, None).unwrap();
    let buckets: Vec<DueBucket> = groups.iter().map(|g| g.bucket).collect();
    assert_eq!(buckets, vec![DueBucket::Overdue, DueBucket::Today, DueBucket::Undated]);
    assert_eq!(tasks.next_step().unwrap().unwrap().title, "Lab report");
    tasks.set_done(a, true).unwrap();
    assert_eq!(tasks.next_step().unwrap().unwrap().title, "Buy a calculator");
    assert_eq!(heard.get(), 4);
    assert!(tasks.save(None, &TaskInput::titled("  ")).is_err());
}

#[test]
fn ideas_become_tasks_come_back_and_can_be_undone() {
    let (tasks, ideas, clock, _) = setup();
    let a = ideas.add("Fix the bike light #bike\nfront one").unwrap();
    let t = ideas.to_task(a, Some(day("2026-09-23"))).unwrap();
    let task = tasks.task(t).unwrap();
    assert_eq!(task.title, "Fix the bike light");
    assert!(task.details.contains("front one"));
    assert!(ideas.ideas(true, "").unwrap().iter().any(|i| i.id == Some(a) && i.done));
    let b = ideas.add("keep me #x").unwrap();
    let kept = ideas.ideas(true, "#x").unwrap()[0].clone();
    ideas.delete(b).unwrap();
    ideas.restore(&kept).unwrap();
    assert_eq!(ideas.all_tags().unwrap(), vec!["x"]);
    clock.0.set(clock.0.get() + chrono::Duration::days(10));
    assert_eq!(ideas.resurfaced(true).unwrap().unwrap().text, "keep me #x");
    assert!(ideas.resurfaced(false).unwrap().is_none());
}

#[test]
fn notes_file_in_a_class_or_a_notebook_and_keep_pictures() {
    use quire::application::notes::NoteService;
    use quire::infrastructure::repositories::{SqliteImages, SqliteNotebooks, SqliteNotes};
    let db = Rc::new(Database::in_memory().unwrap());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(12, 0, 0).unwrap())));
    let notes = NoteService {
        notes: Rc::new(SqliteNotes(db.clone())), courses: Rc::new(SqliteCourses(db.clone())), lookup: Rc::new(SqliteCourses(db.clone())),
        clock, bus: Rc::new(ChangeBus::default()), images: Rc::new(SqliteImages(db.clone())), notebooks: Rc::new(SqliteNotebooks(db)),
    };
    let book = notes.create_notebook("Trips", "#ff0000").unwrap();
    let n = notes.create("# Rome\n\nColosseum", None, "italy", book.id).unwrap();
    assert!(notes.create("x", Some(1), "", book.id).is_err()); // not both
    notes.create("# Florence", None, "Italy", book.id).unwrap();
    assert_eq!(notes.topics(None, book.id).unwrap(), vec!["italy"]); // same spelling reused
    assert_eq!(notes.rename_topic(None, "italy", "Italia", book.id).unwrap(), 2);
    assert_eq!(notes.notebooks().unwrap()[0].1, 2);
    let md = notes.add_image(&[137, 80, 78, 71], "image/png", "a [map]").unwrap();
    assert!(md.starts_with("![a (map)](quire-image:"));
    let updated = notes.update(n.id.unwrap(), &format!("# Rome\n\n{md}"), None, true, "Italia", book.id).unwrap();
    assert_eq!(notes.images_in(&updated.body).unwrap().len(), 1);
    assert_eq!(notes.search("colos", None, None).unwrap().len(), 0); // the edit replaced the body
    assert_eq!(notes.search("rome", None, None).unwrap()[0].title, "Rome");
    notes.delete_notebook(book.id.unwrap()).unwrap();
    assert!(notes.search("", None, None).unwrap().iter().all(|s| s.notebook.is_none()));
}

#[test]
fn courses_events_and_routines_show_on_the_day() {
    use quire::application::planner::{EventInput, ItemKind, PlannerService, RoutineInput};
    use quire::application::timetable::{CourseInput, SlotInput, TimetableService};
    use quire::infrastructure::repositories::*;
    let db = Rc::new(Database::in_memory().unwrap());
    let bus = Rc::new(ChangeBus::default());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(12, 0, 0).unwrap())));
    let timetable = TimetableService { courses: Rc::new(SqliteCourses(db.clone())), bus: bus.clone() };
    let planner = PlannerService {
        courses: Rc::new(SqliteCourses(db.clone())), events: Rc::new(SqliteEvents(db.clone())), tasks: Rc::new(SqliteTasks(db.clone())),
        journal: Rc::new(SqliteJournal(db.clone())), clock, bus, jobs: Rc::new(SqliteJobs(db.clone())),
        shifts: Rc::new(SqliteShifts(db.clone())), routines: Rc::new(SqliteRoutines(db)),
    };
    // 2026-09-23 is a Wednesday (weekday 2)
    let maths = timetable.save_course(None, &CourseInput { name: " Maths ".into(), teacher: "Okafor".into(), room: "B12".into(),
        color: "#4f7cff".into(), slots: vec![SlotInput { weekday: 2, start: 510, end: 560, room: String::new() }] }).unwrap();
    planner.save_event(None, &EventInput { day: day("2026-09-23"), start: 720, end: 765, title: "Lunch".into(), details: String::new(),
        color: "#8a8f98".into(), all_day: false, milestone: false }).unwrap();
    planner.save_routine(None, &RoutineInput { title: "Wake up".into(), minute: 420, weekdays: vec![2, 2, 0], color: "#f5a524".into() }).unwrap();
    planner.save_journal(day("2026-09-23"), "bring the slip").unwrap();
    let a = planner.day_agenda(day("2026-09-23")).unwrap();
    let kinds: Vec<(ItemKind, &str)> = a.items.iter().map(|i| (i.kind, i.title.as_str())).collect();
    assert_eq!(kinds, vec![(ItemKind::Routine, "Wake up"), (ItemKind::Class, "Maths"), (ItemKind::Event, "Lunch")]);
    assert_eq!(a.items[1].room, "B12");
    assert_eq!(a.journal, "bring the slip");
    assert_eq!(planner.routines().unwrap()[0].weekdays, vec![0, 2]);
    assert_eq!(timetable.next_meeting(maths, day("2026-09-23")).unwrap(), Some(day("2026-09-30")));
    timetable.delete_course(maths).unwrap();
    assert!(planner.day_agenda(day("2026-09-23")).unwrap().items.iter().all(|i| i.kind != ItemKind::Class));
}

#[test]
fn water_goal_log_and_reminder() {
    use quire::application::water::{WaterProfile, WaterService};
    use quire::infrastructure::repositories::SqliteWater;
    let db = Rc::new(Database::in_memory().unwrap());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(15, 0, 0).unwrap())));
    let water = WaterService { logs: Rc::new(SqliteWater(db.clone())), settings: Rc::new(SqliteSettings(db.clone())), clock,
                               bus: Rc::new(ChangeBus::default()), last_reminder: Cell::new(None) };
    let goal = water.save_profile(&WaterProfile { height_cm: Some(180.0), weight_kg: Some(75.0), glass_ml: 250, remind: true,
                                                  start: 480, end: 1320, manual_goal: None }).unwrap();
    water.add(250, None).unwrap();
    water.add(250, None).unwrap();
    assert!(water.add(250, Some(day("2026-09-24"))).is_err());
    let o = water.overview(None).unwrap();
    assert_eq!((o.goal, o.drunk, o.entries.len()), (Some(goal), 500, 2));
    assert!(o.expected > 500);
    assert_eq!(water.reminder().unwrap(), Some(format!("500/{goal}")));
    assert_eq!(water.reminder().unwrap(), None); // not again so soon
    water.undo_last(None).unwrap();
    assert_eq!(water.overview(None).unwrap().drunk, 250);
}
