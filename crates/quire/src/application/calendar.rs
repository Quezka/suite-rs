//! Quire's agenda in the computer's own calendar (port of `services/calendar.py`)
//! one way: quire -> file system reads; nothing comes back

use std::rc::Rc;

use chrono::Duration;

use super::errors::AppResult;
use super::planner::{AgendaItem, ItemKind, PlannerService};
use super::ports::{Clock, KeyValueStore, SystemCalendar};
use super::tasks::TaskService;
use crate::domain::ical::{to_ics, CalendarEntry};
use crate::domain::model::MINUTES_PER_DAY;

pub const DAYS_BACK: i64 = 30;
pub const DAYS_AHEAD: i64 = 180;
pub const ENABLED: &str = "calendar.enabled";

fn entry(item: &AgendaItem) -> CalendarEntry {
    let ident = item.ref_id.map_or_else(|| item.title.clone(), |i| i.to_string());
    CalendarEntry {
        uid: format!("{}-{}-{}-{}@quire", item.kind.name(), ident, item.day, item.time.start),
        title: item.title.clone(), day: item.day, start: item.time.start, end: item.time.end,
        all_day: item.is_all_day(), details: item.details.clone(), place: item.room.clone(),
    }
}

pub struct CalendarService {
    pub planner: Rc<PlannerService>,
    pub tasks: Rc<TaskService>,
    pub system: Rc<dyn SystemCalendar>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
}

impl CalendarService {
    pub fn enabled(&self) -> bool {
        self.settings.get(ENABLED).as_deref() == Some("1")
    }

    pub fn can_subscribe(&self) -> bool {
        self.system.can_subscribe()
    }

    pub fn set_enabled(&self, on: bool) -> AppResult {
        self.settings.set(ENABLED, Some(if on { "1" } else { "0" }))?;
        if on {
            self.publish()?;
            self.system.connect()
        } else {
            self.system.disconnect()
        }
    }

    pub fn entries(&self) -> AppResult<Vec<CalendarEntry>> {
        let today = self.clock.today();
        let items = self.planner.items_between(today - Duration::days(DAYS_BACK), today + Duration::days(DAYS_AHEAD))?;
        let mut out: Vec<CalendarEntry> =
            items.iter().filter(|i| !i.is_milestone() && i.kind != ItemKind::Routine).map(entry).collect();
        for group in self.tasks.groups(false, None)? {
            for item in group.items {
                let t = item.task;
                if let Some(due) = t.due {
                    out.push(CalendarEntry {
                        uid: format!("task-{}@quire", t.id.unwrap_or_default()), title: t.title, day: due,
                        start: 0, end: MINUTES_PER_DAY, all_day: true, details: t.details, place: String::new(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// write file if on; cheap if same
    pub fn publish(&self) -> AppResult {
        if self.enabled() {
            self.system.publish(&to_ics(&self.entries()?, "Quire", self.clock.now()))?;
        }
        Ok(())
    }

    /// hand file to calendar app (windows = import)
    pub fn open(&self) -> AppResult {
        self.settings.set(ENABLED, Some("1"))?;
        self.publish()?;
        self.system.open()
    }
}
