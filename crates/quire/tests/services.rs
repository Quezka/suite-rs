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

#[test]
fn medicines_plan_mark_and_nudge_once() {
    use quire::application::medicine::MedicineService;
    use quire::domain::medicine::Medicine;
    use quire::infrastructure::repositories::SqliteMedicines;
    let db = Rc::new(Database::in_memory().unwrap());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(9, 5, 0).unwrap())));
    let meds = MedicineService { repo: Rc::new(SqliteMedicines(db)), clock, bus: Rc::new(ChangeBus::default()),
                                 reminded: Default::default() };
    let id = meds.save(None, &Medicine { name: " Vitamin D ".into(), dose: "1 tab".into(), times: vec![1200, 540],
        weekdays: (0..7).collect(), active: true, notes: String::new(), id: None }).unwrap();
    let pending = meds.pending().unwrap();
    assert_eq!(pending.iter().map(|d| d.planned).collect::<Vec<_>>(), vec![540, 1200]);
    assert_eq!(meds.due().unwrap().len(), 1);
    assert!(meds.due().unwrap().is_empty()); // once
    meds.mark(id, day("2026-09-23"), 540, true).unwrap();
    meds.mark(id, day("2026-09-23"), 540, true).unwrap(); // again: changes, doesn't add
    let today = meds.day(None).unwrap();
    assert_eq!(today.iter().filter(|d| d.done).count(), 1);
    assert_eq!(meds.medicine(id).unwrap().name, "Vitamin D");
    let (planned, taken) = meds.week().unwrap();
    assert!(taken == 1 && planned >= 1);
}

#[test]
fn sleep_one_tap_night_goal_and_overview() {
    use quire::application::sleep::SleepService;
    use quire::infrastructure::repositories::{SqliteFocusLog, SqliteSleep};
    let db = Rc::new(Database::in_memory().unwrap());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-22").and_hms_opt(23, 10, 0).unwrap())));
    let sleep = SleepService { logs: Rc::new(SqliteSleep(db.clone())), settings: Rc::new(SqliteSettings(db.clone())), clock: clock.clone(),
                               bus: Rc::new(ChangeBus::default()), focus: Some(Rc::new(SqliteFocusLog(db))), reminded_on: Cell::new(None) };
    assert_eq!(sleep.goal(), (8.0, 360));
    assert!(sleep.wake_up().is_err());
    sleep.go_to_sleep().unwrap();
    clock.0.set(day("2026-09-23").and_hms_opt(6, 40, 0).unwrap());
    let night = sleep.wake_up().unwrap();
    assert_eq!((night.asleep, night.woke, night.day), (23 * 60 + 10, 6 * 60 + 40, day("2026-09-23")));
    sleep.rate(day("2026-09-23"), 4).unwrap();
    let o = sleep.overview(14).unwrap();
    assert_eq!(o.last_night.unwrap().quality, Some(4));
    assert_eq!(o.days.len(), 14);
    assert!(o.asleep_since.is_none());
    sleep.save_goal(7.5, 7 * 60).unwrap();
    assert_eq!(sleep.goal(), (7.5, 420));
    clock.0.set(day("2026-09-23").and_hms_opt(23, 10, 0).unwrap()); // bedtime 23:30: wind down
    assert_eq!(sleep.reminder(), Some(23 * 60 + 30));
    assert_eq!(sleep.reminder(), None); // once a day
}

#[test]
fn focus_logs_sessions_and_a_block_becomes_an_event() {
    use quire::application::focus::FocusService;
    use quire::domain::focus::Phase;
    use quire::infrastructure::repositories::{SqliteEvents, SqliteFocusLog};
    let db = Rc::new(Database::in_memory().unwrap());
    let clock = Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(9, 0, 0).unwrap())));
    let bus = Rc::new(ChangeBus::default());
    let tasks = TaskService::new(Rc::new(SqliteTasks(db.clone())), Rc::new(SqliteCourses(db.clone())), clock.clone(), bus.clone());
    let essay = tasks.save(None, &TaskInput::titled("History essay")).unwrap();
    let focus = FocusService::new(Rc::new(SqliteFocusLog(db.clone())), Rc::new(SqliteSettings(db.clone())), Rc::new(SqliteTasks(db.clone())),
                                  Rc::new(SqliteCourses(db.clone())), clock.clone(), bus, Some(Rc::new(SqliteEvents(db.clone()))));
    focus.set_focus_task(Some(essay)).unwrap();
    focus.toggle();
    clock.0.set(clock.0.get() + chrono::Duration::minutes(25));
    let ended = focus.tick().unwrap().unwrap();
    assert_eq!((ended.phase, ended.next_phase), (Phase::Work, Phase::ShortBreak));
    let stats = focus.stats().unwrap();
    assert_eq!((stats.today_sessions, stats.today_minutes), (1, 25));
    assert_eq!(stats.today_by_project, vec![("History essay".to_string(), 25)]);
    focus.start_block(60).unwrap();
    for _ in 0..61 {
        clock.0.set(clock.0.get() + chrono::Duration::minutes(1));
        focus.tick().unwrap();
    }
    assert!(focus.state().block_left_seconds.is_none());
    let events = SqliteEvents(db);
    use quire::application::ports::EventRepository;
    let week = events.between(day("2026-09-23"), day("2026-09-23")).unwrap();
    assert_eq!(week.len(), 1);
    assert_eq!((week[0].title.as_str(), week[0].time.start, week[0].time.end), ("Focus: History essay", 565, 625));
}
