//! Read-only use cases over what was synced from the school register (port of `services/school_records.py`):
//! grades and averages, terms, homework and tests, lessons, absences, noticeboard, textbooks, documents.

use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate};

use super::errors::{AppError, AppResult};
use super::ports::{Clock, CourseRepository, SchoolLinks, SchoolRecordRepository};
use super::tasks::TaskItem;
use crate::domain::model::{average, Course, Grade, Lesson, Subject, Task, TaskKind, PASS_MARK};
use crate::domain::register::{lesson_hours, needed_grade, running_average, Absence, AbsenceKind, Book, Notice, SchoolDocument,
                              ABSENCE_LIMIT, GRADE_MAX, GRADE_MIN};
use crate::domain::ValidationError;

pub const SCHOOL_WEEKS: i32 = 33; // no register calendar: a typical Italian school year
pub const MAX_LESSON_HOUR: i32 = 10;

#[derive(Clone, Debug)]
pub struct SubjectGrades {
    pub subject: String,
    pub course: Option<Course>,
    /// Newest first.
    pub grades: Vec<Grade>,
    pub average: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct SchoolOverview {
    pub average: Option<f64>,
    pub grade_count: usize,
    pub next_test: Option<TaskItem>,
    pub homework_due_this_week: usize,
    pub next_homework: Option<TaskItem>,
    pub below_pass: Vec<SubjectGrades>,
}

#[derive(Clone, Debug)]
pub struct AbsenceSummary {
    /// (absence, hours missed), newest first.
    pub items: Vec<(Absence, i32)>,
    pub absent_days: usize,
    pub late_entries: usize,
    pub early_exits: usize,
    pub unjustified: usize,
    pub hours_missed: i32,
    /// Lesson hours in the year (estimated); 0 without a timetable.
    pub school_hours: i32,
    pub limit_hours: i32,
}

impl AbsenceSummary {
    pub fn share(&self) -> Option<f64> {
        (self.school_hours > 0).then(|| f64::from(self.hours_missed) / f64::from(self.school_hours))
    }
    pub fn hours_left(&self) -> i32 {
        (self.limit_hours - self.hours_missed).max(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NeededGrade {
    pub subject: String,
    pub target: f64,
    pub tests: u32,
    pub mark: f64,
    pub average: Option<f64>,
    pub grade_count: usize,
}

impl NeededGrade {
    pub fn already_safe(&self) -> bool {
        self.mark <= GRADE_MIN
    }
    pub fn reachable(&self) -> bool {
        self.mark <= GRADE_MAX
    }
}

pub struct SchoolRecordsService {
    pub courses: Rc<dyn CourseRepository>,
    pub links: Rc<dyn SchoolLinks>,
    pub records: Rc<dyn SchoolRecordRepository>,
    pub clock: Rc<dyn Clock>,
    /// Prefix of the register's sync ids, e.g. "classeviva".
    pub source: String,
}

fn task_order(a: &Task, b: &Task) -> std::cmp::Ordering {
    (a.due, a.kind != TaskKind::Exam, a.title.to_lowercase()).cmp(&(b.due, b.kind != TaskKind::Exam, b.title.to_lowercase()))
}

impl SchoolRecordsService {
    fn course_of(courses: &[Course], id: Option<i64>) -> Option<Course> {
        id.and_then(|i| courses.iter().find(|c| c.id == Some(i)).cloned())
    }

    /// Subjects seen in the last sync, each with the course it feeds into.
    pub fn subject_links(&self) -> AppResult<Vec<(Subject, Option<Course>)>> {
        let courses = self.courses.list()?;
        Ok(self.records.subjects()?.into_iter().map(|s| {
            let c = courses.iter().find(|c| c.external_id.as_deref() == Some(s.external_id.as_str())).cloned();
            (s, c)
        }).collect())
    }

    /// School terms the grades belong to, in calendar order.
    pub fn periods(&self) -> AppResult<Vec<String>> {
        let mut first: HashMap<String, NaiveDate> = HashMap::new();
        for g in self.records.grades()? {
            if !g.period.is_empty() {
                let e = first.entry(g.period.clone()).or_insert(g.day);
                if g.day < *e {
                    *e = g.day;
                }
            }
        }
        let mut out: Vec<(String, NaiveDate)> = first.into_iter().collect();
        out.sort_by_key(|(_, d)| *d);
        Ok(out.into_iter().map(|(p, _)| p).collect())
    }

    fn grades(&self, period: Option<&str>) -> AppResult<Vec<Grade>> {
        Ok(self.records.grades()?.into_iter().filter(|g| period.is_none_or(|p| g.period == p)).collect())
    }

    pub fn grades_by_subject(&self, period: Option<&str>) -> AppResult<Vec<SubjectGrades>> {
        let courses = self.courses.list()?;
        let mut groups: Vec<(String, Vec<Grade>)> = Vec::new();
        for g in self.grades(period)? {
            match groups.iter_mut().find(|(s, _)| *s == g.subject) {
                Some((_, v)) => v.push(g),
                None => groups.push((g.subject.clone(), vec![g])),
            }
        }
        groups.sort_by_key(|(s, _)| s.to_lowercase());
        Ok(groups.into_iter().map(|(subject, mut grades)| {
            grades.sort_by(|a, b| b.day.cmp(&a.day));
            let course = grades.iter().find_map(|g| Self::course_of(&courses, g.course_id));
            SubjectGrades { average: average(&grades), subject, course, grades }
        }).collect())
    }

    pub fn overall_average(&self, period: Option<&str>) -> AppResult<Option<f64>> {
        Ok(average(&self.grades(period)?))
    }

    fn register_tasks(&self) -> AppResult<Vec<Task>> {
        self.links.tasks_by_external_prefix(&format!("{}:", self.source))
    }

    /// Homework and tests from the register still to do, soonest first.
    pub fn upcoming(&self, days: i64) -> AppResult<Vec<TaskItem>> {
        let today = self.clock.today();
        let courses = self.courses.list()?;
        let mut tasks: Vec<Task> = self.register_tasks()?.into_iter()
            .filter(|t| !t.done && t.due.is_some_and(|d| (0..=days).contains(&(d - today).num_days()))).collect();
        tasks.sort_by(task_order);
        Ok(tasks.into_iter().map(|t| TaskItem { course: Self::course_of(&courses, t.course_id), overdue: false, task: t }).collect())
    }

    /// Everything the register assigned: overdue work, today and what's ahead (with done ones if asked).
    pub fn agenda(&self, include_done: bool, days_ahead: i64, done_days_back: i64) -> AppResult<Vec<TaskItem>> {
        let today = self.clock.today();
        let courses = self.courses.list()?;
        let mut tasks: Vec<Task> = self.register_tasks()?.into_iter().filter(|t| {
            let Some(due) = t.due else { return false };
            let ahead = (due - today).num_days();
            if t.done { include_done && -done_days_back <= ahead && ahead <= days_ahead } else { ahead <= days_ahead }
        }).collect();
        tasks.sort_by(task_order);
        Ok(tasks.into_iter().map(|t| TaskItem { course: Self::course_of(&courses, t.course_id), overdue: t.is_overdue(today), task: t }).collect())
    }

    pub fn overview(&self, period: Option<&str>) -> AppResult<SchoolOverview> {
        let today = self.clock.today();
        let soon = self.upcoming(60)?;
        let tests: Vec<&TaskItem> = soon.iter().filter(|i| i.task.kind == TaskKind::Exam).collect();
        let homework: Vec<&TaskItem> = soon.iter().filter(|i| i.task.kind != TaskKind::Exam).collect();
        let subjects = self.grades_by_subject(period)?;
        Ok(SchoolOverview {
            average: self.overall_average(period)?,
            grade_count: subjects.iter().flat_map(|s| &s.grades).filter(|g| g.counts()).count(),
            next_test: tests.first().map(|i| (*i).clone()),
            homework_due_this_week: homework.iter().filter(|i| i.task.due.is_some_and(|d| (d - today).num_days() <= 6)).count(),
            next_homework: homework.first().map(|i| (*i).clone()),
            below_pass: subjects.into_iter().filter(|s| s.average.is_some_and(|a| a < PASS_MARK)).collect(),
        })
    }

    pub fn recent_lessons(&self, days: i64) -> AppResult<Vec<Lesson>> {
        let today = self.clock.today();
        let mut lessons = self.records.lessons_between(today - Duration::days(days), today)?;
        lessons.sort_by(|a, b| (b.day, b.hour).cmp(&(a.day, a.hour)));
        Ok(lessons)
    }

    /// Lesson hours on each weekday, from the timetable in Quire.
    fn hours_per_weekday(&self) -> AppResult<HashMap<u8, i32>> {
        let mut hours = HashMap::new();
        for c in self.courses.list()? {
            for s in &c.slots {
                *hours.entry(s.weekday).or_default() += lesson_hours(s.time.end - s.time.start);
            }
        }
        Ok(hours)
    }

    /// Absences so far and how close they are to the limit of a quarter of the year.
    pub fn absence_summary(&self) -> AppResult<AbsenceSummary> {
        let per_day = self.hours_per_weekday()?;
        let wd = |d: NaiveDate| d.weekday().num_days_from_monday() as u8;
        let days = self.records.school_days()?;
        let school_hours = if days.is_empty() { per_day.values().sum::<i32>() * SCHOOL_WEEKS }
                           else { days.iter().map(|d| *per_day.get(&wd(*d)).unwrap_or(&0)).sum() };
        let mut absences = self.records.absences()?;
        absences.sort_by(|a, b| b.day.cmp(&a.day));
        let items: Vec<(Absence, i32)> = absences.iter().map(|a| (a.clone(), a.hours_missed(*per_day.get(&wd(a.day)).unwrap_or(&0)))).collect();
        Ok(AbsenceSummary {
            absent_days: absences.iter().filter(|a| a.kind == AbsenceKind::Absent).count(),
            late_entries: absences.iter().filter(|a| matches!(a.kind, AbsenceKind::Late | AbsenceKind::ShortLate)).count(),
            early_exits: absences.iter().filter(|a| a.kind == AbsenceKind::EarlyExit).count(),
            unjustified: absences.iter().filter(|a| !a.justified).count(),
            hours_missed: items.iter().map(|(_, h)| h).sum(),
            limit_hours: (f64::from(school_hours) * ABSENCE_LIMIT) as i32,
            school_hours, items,
        })
    }

    /// The lesson hour of a late entry or early exit the register left blank (None forgets it); kept across syncs.
    pub fn set_absence_hour(&self, id: &str, hour: Option<i32>) -> AppResult {
        if hour.is_some_and(|h| !(1..=MAX_LESSON_HOUR).contains(&h)) {
            return Err(ValidationError("The hour must be between 1 and 10.").into());
        }
        let mut absences = self.records.absences()?;
        let a = absences.iter_mut().find(|a| a.external_id == id)
            .ok_or_else(|| AppError::NotFound("That absence isn't in the register any more. Sync again.".into()))?;
        a.own_hour = hour;
        self.records.replace_absences(&absences)
    }

    pub fn notices(&self) -> AppResult<Vec<Notice>> {
        self.records.notices()
    }

    pub fn unread_notices(&self) -> AppResult<usize> {
        Ok(self.records.notices()?.iter().filter(|n| !n.read).count())
    }

    /// Textbooks, grouped by subject.
    pub fn books(&self) -> AppResult<Vec<Book>> {
        let mut b = self.records.books()?;
        b.sort_by_key(|x| (x.subject.to_lowercase(), x.title.to_lowercase()));
        Ok(b)
    }

    pub fn documents(&self) -> AppResult<Vec<SchoolDocument>> {
        self.records.documents()
    }

    /// The mark needed on the next `tests` tests for the subject's average to reach `target`.
    pub fn needed(&self, subject: &str, target: f64, tests: u32, period: Option<&str>) -> AppResult<NeededGrade> {
        let marks: Vec<Grade> = self.grades(period)?.into_iter().filter(|g| g.subject == subject && g.counts()).collect();
        let values: Vec<f64> = marks.iter().filter_map(|g| g.value).collect();
        Ok(NeededGrade { subject: subject.into(), target, tests, mark: needed_grade(&values, target, tests),
                         average: average(&marks), grade_count: marks.len() })
    }

    /// How the average moved, mark by mark: one subject, or all together.
    pub fn trend(&self, subject: Option<&str>, period: Option<&str>) -> AppResult<Vec<(NaiveDate, f64)>> {
        let points: Vec<(NaiveDate, f64)> = self.grades(period)?.into_iter()
            .filter(|g| g.counts() && subject.is_none_or(|s| g.subject == s)).filter_map(|g| g.value.map(|v| (g.day, v))).collect();
        Ok(running_average(&points))
    }
}
