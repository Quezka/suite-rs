//! Courses and their weekly class times (port of `services/timetable.py`).

use std::rc::Rc;

use chrono::NaiveDate;

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::CourseRepository;
use crate::domain::model::{ClassSlot, Course, TimeRange};

#[derive(Clone, Debug)]
pub struct SlotInput {
    pub weekday: u8,
    pub start: i32,
    pub end: i32,
    pub room: String,
}

#[derive(Clone, Debug)]
pub struct CourseInput {
    pub name: String,
    pub teacher: String,
    pub room: String,
    pub color: String,
    pub slots: Vec<SlotInput>,
}

pub struct TimetableService {
    pub courses: Rc<dyn CourseRepository>,
    pub bus: Rc<ChangeBus>,
}

impl TimetableService {
    pub fn courses(&self) -> AppResult<Vec<Course>> {
        self.courses.list()
    }

    pub fn course(&self, id: i64) -> AppResult<Course> {
        self.courses.list()?.into_iter().find(|c| c.id == Some(id))
            .ok_or_else(|| AppError::NotFound(format!("Course {id} does not exist.")))
    }

    /// Create a course, or change one; its register link is kept.
    pub fn save_course(&self, id: Option<i64>, data: &CourseInput) -> AppResult<i64> {
        let mut slots = Vec::new();
        for s in &data.slots {
            slots.push(ClassSlot::new(s.weekday, TimeRange::new(s.start, s.end)?, &s.room)?);
        }
        let mut course = match id {
            None => Course::named(""),
            Some(id) => self.course(id)?,
        };
        course.name = data.name.trim().into();
        course.teacher = data.teacher.trim().into();
        course.room = data.room.trim().into();
        course.color = data.color.clone();
        course.slots = slots;
        course.validate()?;
        let id = match course.id {
            None => self.courses.add(&course)?,
            Some(id) => {
                self.courses.update(&course)?;
                id
            }
        };
        self.bus.publish(Topic::Courses);
        Ok(id)
    }

    /// Delete a course. Its notes and tasks are kept but unlinked.
    pub fn delete_course(&self, id: i64) -> AppResult {
        self.courses.delete(id)?;
        self.bus.publish(Topic::Courses);
        Ok(())
    }

    pub fn next_meeting(&self, id: i64, after: NaiveDate) -> AppResult<Option<NaiveDate>> {
        Ok(self.course(id)?.next_meeting_after(after))
    }
}
