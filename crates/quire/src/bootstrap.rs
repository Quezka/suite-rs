//! composition root: only module knowing every layer (= python `bootstrap.py`)

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use crate::application::bus::ChangeBus;
use crate::application::calendar::CalendarService;
use crate::application::digest::DigestService;
use crate::application::focus::FocusService;
use crate::application::ideas::IdeaService;
use crate::application::medicine::MedicineService;
use crate::application::notes::NoteService;
use crate::application::planner::PlannerService;
use crate::application::ports::{
    CloudBackend, Clock, CredentialStore, LoginItem, ReleaseFeed, SchoolRegister, SystemCalendar, UpdateInstaller,
};
use crate::application::quiet::{QuietService, QuietSwitch};
use crate::application::reminders::ReminderService;
use crate::application::school::SchoolSyncService;
use crate::application::school_records::SchoolRecordsService;
use crate::application::sleep::SleepService;
use crate::application::startup::StartupService;
use crate::application::sync::SyncService;
use crate::application::tasks::TaskService;
use crate::application::timetable::TimetableService;
use crate::application::updates::UpdateService;
use crate::application::services::Services;
use crate::application::water::WaterService;
use crate::application::work::WorkService;
use crate::infrastructure::classeviva::ClassevivaRegister;
use crate::infrastructure::clock::SystemClock;
use crate::infrastructure::cloud::RoutingCloud;
use crate::infrastructure::credentials::KeyringCredentials;
use crate::infrastructure::http::UreqHttp;
use crate::infrastructure::login_item::SystemLoginItem;
use crate::infrastructure::quiet::platform_quiet;
use crate::infrastructure::repositories::*;
use crate::infrastructure::sqlite::Database;
use crate::infrastructure::system_calendar::platform_calendar;
use crate::infrastructure::updates::{platform_installer, GitHubReleaseFeed};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO: &str = "Quezka/quire";
/// name sent to quire-server w/ sign-in
pub const DEVICE: &str = "Quire desktop";


/// swap outside world in tests | demo; none = real thing
#[derive(Default)]
pub struct Overrides {
    pub clock: Option<Rc<dyn Clock>>,
    pub register: Option<Arc<dyn SchoolRegister>>,
    pub credentials: Option<Rc<dyn CredentialStore>>,
    pub cloud: Option<Arc<dyn CloudBackend>>,
    pub sync_secrets: Option<Rc<dyn CredentialStore>>,
    pub releases: Option<Arc<dyn ReleaseFeed>>,
    pub installer: Option<Arc<dyn UpdateInstaller>>,
    pub login_item: Option<Rc<dyn LoginItem>>,
    pub quiet: Option<Rc<dyn QuietSwitch>>,
    pub calendar: Option<Rc<dyn SystemCalendar>>,
}

/// services + db handle (db = backup, close)
pub fn build_services(db_path: &Path, o: Overrides) -> rusqlite::Result<(Services, Rc<Database>)> {
    let db = Rc::new(Database::open(db_path)?);
    Ok((wire(db.clone(), o), db))
}

pub fn wire(db: Rc<Database>, o: Overrides) -> Services {
    let clock: Rc<dyn Clock> = o.clock.unwrap_or_else(|| Rc::new(SystemClock));
    let bus = Rc::new(ChangeBus::default());
    let settings = Rc::new(SqliteSettings(db.clone()));
    let courses = Rc::new(SqliteCourses(db.clone()));
    let tasks = Rc::new(SqliteTasks(db.clone()));
    let events = Rc::new(SqliteEvents(db.clone()));
    let school = Rc::new(SqliteSchool(db.clone()));
    let focus_log = Rc::new(SqliteFocusLog(db.clone()));
    let register: Arc<dyn SchoolRegister> = o.register.unwrap_or_else(|| Arc::new(ClassevivaRegister::new(Arc::new(UreqHttp::default()))));
    let planner = Rc::new(PlannerService {
        courses: courses.clone(), events: events.clone(), tasks: tasks.clone(), journal: Rc::new(SqliteJournal(db.clone())),
        clock: clock.clone(), bus: bus.clone(), jobs: Rc::new(SqliteJobs(db.clone())), shifts: Rc::new(SqliteShifts(db.clone())),
        routines: Rc::new(SqliteRoutines(db.clone())),
    });
    let tasks_service = Rc::new(TaskService::new(tasks.clone(), courses.clone(), clock.clone(), bus.clone()));
    let ideas = Rc::new(IdeaService::new(Rc::new(SqliteIdeas(db.clone())), clock.clone(), bus.clone(), tasks_service.clone(), settings.clone()));
    let source = register.name().to_lowercase();
    Services {
        timetable: TimetableService { courses: courses.clone(), bus: bus.clone() },
        notes: NoteService {
            notes: Rc::new(SqliteNotes(db.clone())), courses: courses.clone(), lookup: courses.clone(), clock: clock.clone(),
            bus: bus.clone(), images: Rc::new(SqliteImages(db.clone())), notebooks: Rc::new(SqliteNotebooks(db.clone())),
        },
        school: SchoolRecordsService { courses: courses.clone(), links: school.clone(), records: school.clone(), clock: clock.clone(), source },
        school_sync: SchoolSyncService {
            register, credentials: o.credentials.unwrap_or_else(|| Rc::new(KeyringCredentials { account: "classeviva".into() })),
            courses: courses.clone(), lookup: courses.clone(), tasks: tasks.clone(), records: school.clone(), links: school,
            settings: settings.clone(), clock: clock.clone(), bus: bus.clone(),
        },
        work: WorkService { jobs: Rc::new(SqliteJobs(db.clone())), shifts: Rc::new(SqliteShifts(db.clone())), clock: clock.clone(), bus: bus.clone() },
        focus: FocusService::new(focus_log.clone(), settings.clone(), tasks, courses, clock.clone(), bus.clone(), Some(events)),
        sleep: SleepService {
            logs: Rc::new(SqliteSleep(db.clone())), settings: settings.clone(), clock: clock.clone(), bus: bus.clone(),
            focus: Some(focus_log), reminded_on: Cell::new(None),
        },
        water: WaterService { logs: Rc::new(SqliteWater(db.clone())), settings: settings.clone(), clock: clock.clone(), bus: bus.clone(), last_reminder: Cell::new(None) },
        medicine: MedicineService { repo: Rc::new(SqliteMedicines(db.clone())), clock: clock.clone(), bus: bus.clone(), reminded: RefCell::default() },
        digest: DigestService { planner: planner.clone(), tasks: tasks_service.clone(), ideas: ideas.clone(), settings: settings.clone(), clock: clock.clone() },
        calendar: CalendarService {
            planner: planner.clone(), tasks: tasks_service.clone(),
            system: o.calendar.unwrap_or_else(|| Rc::from(platform_calendar())), settings: settings.clone(), clock: clock.clone(),
        },
        quiet: QuietService { planner: planner.clone(), settings: settings.clone(), clock: clock.clone(), switch: o.quiet.unwrap_or_else(|| Rc::from(platform_quiet())) },
        reminders: ReminderService { planner: planner.clone(), settings: settings.clone(), clock: clock.clone(), sent: RefCell::default() },
        sync: SyncService {
            store: Rc::new(SqliteSync(db.clone())), cloud: o.cloud.unwrap_or_else(|| Arc::new(RoutingCloud::new(VERSION, DEVICE))),
            secrets: o.sync_secrets.unwrap_or_else(|| Rc::new(KeyringCredentials { account: "sync".into() })),
            settings: settings.clone(), clock: clock.clone(), bus: bus.clone(),
        },
        updates: UpdateService {
            feed: o.releases.unwrap_or_else(|| Arc::new(GitHubReleaseFeed::new(REPO, VERSION))),
            installer: o.installer.unwrap_or_else(|| Arc::from(platform_installer())),
            settings: settings.clone(), clock: clock.clone(), current_version: VERSION.into(),
        },
        startup: StartupService { login_item: o.login_item.unwrap_or_else(|| Rc::new(SystemLoginItem::for_this_exe())), settings },
        planner,
        tasks: tasks_service,
        ideas,
        bus,
    }
}
