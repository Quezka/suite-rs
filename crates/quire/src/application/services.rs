//! every use case of quire, one place (= python `application/services/__init__.py`); built @ `bootstrap.rs`

use std::rc::Rc;

use super::calendar::CalendarService;
use super::digest::DigestService;
use super::focus::FocusService;
use super::ideas::IdeaService;
use super::medicine::MedicineService;
use super::notes::NoteService;
use super::planner::PlannerService;
use super::quiet::QuietService;
use super::reminders::ReminderService;
use super::school::SchoolSyncService;
use super::school_records::SchoolRecordsService;
use super::sleep::SleepService;
use super::startup::StartupService;
use super::sync::SyncService;
use super::tasks::TaskService;
use super::timetable::TimetableService;
use super::updates::UpdateService;
use super::water::WaterService;
use super::work::WorkService;
use super::bus::ChangeBus;

/// every use case on one db
pub struct Services {
    pub timetable: TimetableService,
    pub planner: Rc<PlannerService>,
    pub tasks: Rc<TaskService>,
    pub notes: NoteService,
    pub school: SchoolRecordsService,
    pub school_sync: SchoolSyncService,
    pub work: WorkService,
    pub focus: FocusService,
    pub sleep: SleepService,
    pub water: WaterService,
    pub medicine: MedicineService,
    pub ideas: Rc<IdeaService>,
    pub digest: DigestService,
    pub calendar: CalendarService,
    pub quiet: QuietService,
    pub reminders: ReminderService,
    pub sync: SyncService,
    pub updates: UpdateService,
    pub startup: StartupService,
    pub bus: Rc<ChangeBus>,
}
