//! School-register records beyond grades and lessons: absences, the noticeboard, textbooks and documents, plus the
//! arithmetic Quire does on them (port of `quire/domain/register.py`).

use chrono::NaiveDate;

/// Italian rule: missing more than a quarter of the year's lesson hours can mean repeating it (schools grant
/// exceptions, e.g. for health reasons).
pub const ABSENCE_LIMIT: f64 = 0.25;
pub const GRADE_MIN: f64 = 1.0;
pub const GRADE_MAX: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbsenceKind {
    /// The whole day.
    Absent,
    /// Came in after the first hour.
    Late,
    /// A few minutes late: no hours lost.
    ShortLate,
    EarlyExit,
}

impl AbsenceKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Late => "late",
            Self::ShortLate => "short_late",
            Self::EarlyExit => "early_exit",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Absent, Self::Late, Self::ShortLate, Self::EarlyExit].into_iter().find(|k| k.code() == code)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Absence {
    pub external_id: String,
    pub day: NaiveDate,
    pub kind: AbsenceKind,
    /// The lesson hour of a late entry or early exit.
    pub hour: Option<i32>,
    pub justified: bool,
    pub reason: String,
    /// Hours the register itself counted, when it says (0: unknown).
    pub hours: i32,
    /// The hour you entered yourself, for when the school recorded the late entry or early exit without one
    /// (Classeviva then sends no hour at all).
    pub own_hour: Option<i32>,
}

impl Absence {
    pub fn known_hour(&self) -> Option<i32> {
        self.hour.or(self.own_hour)
    }

    /// Lesson hours lost, given how many hours that day has (from the timetable).
    pub fn hours_missed(&self, hours_that_day: i32) -> i32 {
        if self.hours != 0 {
            return self.hours;
        }
        match self.kind {
            AbsenceKind::Absent => hours_that_day,
            // "Entered at the 2nd hour": missed the 1st.
            AbsenceKind::Late => (self.known_hour().unwrap_or(2) - 1).max(0),
            // "Left at the 5th hour": missed the 5th onwards.
            AbsenceKind::EarlyExit => (hours_that_day - self.known_hour().unwrap_or(hours_that_day) + 1).max(0),
            AbsenceKind::ShortLate => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NoticeAttachment {
    pub number: i32,
    pub file_name: String,
}

/// A circular or announcement on the school noticeboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub external_id: String,
    /// The register's event code and publication id, needed to open it.
    pub code: String,
    pub pub_id: String,
    pub title: String,
    pub category: String,
    pub published: NaiveDate,
    pub valid_until: Option<NaiveDate>,
    pub read: bool,
    pub attachments: Vec<NoticeAttachment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub isbn: String,
    pub title: String,
    pub subject: String,
    pub author: String,
    pub publisher: String,
    pub volume: String,
    pub price: Option<f64>,
    pub to_buy: bool,
    pub owned: bool,
    pub new_adoption: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DocumentKind {
    /// A file the school published for the student.
    Document,
    /// A term report card (pagella).
    Report,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchoolDocument {
    /// The register's id (a hash for documents).
    pub external_id: String,
    pub title: String,
    pub kind: DocumentKind,
    /// Reports open as a web page.
    pub link: String,
}

/// Italian "ore" in a class period: a 50-60 minute period is one, a double period two.
pub fn lesson_hours(minutes: i32) -> i32 {
    if minutes <= 0 {
        return 0;
    }
    ((f64::from(minutes) / 60.0).round_ties_even() as i32).max(1)
}

/// The mark needed on each of the next `tests` tests for the average to reach `target`. May fall below the lowest
/// mark (the target is safe whatever happens) or above the highest (it can't be reached with that many tests);
/// callers say so.
pub fn needed_grade(values: &[f64], target: f64, tests: u32) -> f64 {
    let count = values.len() as f64;
    let tests = f64::from(tests);
    let needed = (target * (count + tests) - values.iter().sum::<f64>()) / tests;
    // Marks go in quarters (6+, 6½, 6-…): round up.
    (needed * 4.0 - 1e-9).ceil() / 4.0
}

/// (day, average of every mark up to that day), one point per day, oldest first.
pub fn running_average(points: &[(NaiveDate, f64)]) -> Vec<(NaiveDate, f64)> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut result: Vec<(NaiveDate, f64)> = Vec::new();
    let mut total = 0.0;
    for (n, (day, value)) in sorted.into_iter().enumerate() {
        total += value;
        let entry = (day, super::model::round2(total / (n + 1) as f64));
        match result.last_mut() {
            Some(last) if last.0 == day => *last = entry,
            _ => result.push(entry),
        }
    }
    result
}
