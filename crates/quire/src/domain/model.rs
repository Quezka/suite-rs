//! Core entities and business rules (port of `quire/domain/model.py`).
//!
//! Times of day are minutes after midnight; weekdays count from Monday = 0 to Sunday = 6, as in Python.

use std::collections::HashSet;
use std::sync::OnceLock;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use fancy_regex::Regex;

use super::errors::{Checked, ValidationError};

pub const MINUTES_PER_DAY: i32 = 24 * 60;
/// A milestone is stored as an event one minute long.
pub const MILESTONE_MINUTES: i32 = 1;
pub const UPCOMING_DAYS: i64 = 7;
pub const COURSE_COLORS: [&str; 10] =
    ["#4f7cff", "#e5484d", "#30a46c", "#f5a524", "#8e4ec6", "#12a594", "#e93d82", "#f76b15", "#0090ff", "#978365"];

fn err<T>(message: &'static str) -> Checked<T> {
    Err(ValidationError(message))
}

/// Monday = 0 ... Sunday = 6.
pub fn weekday(day: NaiveDate) -> u8 {
    day.weekday().num_days_from_monday() as u8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeRange {
    pub start: i32,
    pub end: i32,
}

impl TimeRange {
    pub fn new(start: i32, end: i32) -> Checked<Self> {
        if !((0..MINUTES_PER_DAY).contains(&start) && 0 < end && end <= MINUTES_PER_DAY) {
            return err("Times must fall within a single day.");
        }
        if end <= start {
            return err("The end time must be after the start time.");
        }
        Ok(Self { start, end })
    }

    pub fn duration(&self) -> i32 {
        self.end - self.start
    }
}

/// One weekly meeting of a course.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassSlot {
    pub weekday: u8,
    pub time: TimeRange,
    pub room: String,
}

impl ClassSlot {
    pub fn new(weekday: u8, time: TimeRange, room: &str) -> Checked<Self> {
        if weekday > 6 {
            return err("Weekday must be between Monday and Sunday.");
        }
        Ok(Self { weekday, time, room: room.to_string() })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Course {
    pub name: String,
    pub teacher: String,
    pub room: String,
    pub color: String,
    pub slots: Vec<ClassSlot>,
    pub id: Option<i64>,
    /// Set when the course came from a school register.
    pub external_id: Option<String>,
}

impl Course {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            teacher: String::new(),
            room: String::new(),
            color: "#4f7cff".into(),
            slots: Vec::new(),
            id: None,
            external_id: None,
        }
    }

    pub fn validate(&self) -> Checked {
        if self.name.trim().is_empty() {
            return err("Give the course a name.");
        }
        Ok(())
    }

    pub fn slots_on(&self, weekday: u8) -> Vec<&ClassSlot> {
        let mut slots: Vec<&ClassSlot> = self.slots.iter().filter(|s| s.weekday == weekday).collect();
        slots.sort_by_key(|s| s.time.start);
        slots
    }

    pub fn room_for<'a>(&'a self, slot: &'a ClassSlot) -> &'a str {
        if slot.room.is_empty() { &self.room } else { &slot.room }
    }

    pub fn next_meeting_after(&self, day: NaiveDate) -> Option<NaiveDate> {
        let days: HashSet<u8> = self.slots.iter().map(|s| s.weekday).collect();
        (1..8).map(|offset| day + Duration::days(offset)).find(|d| days.contains(&weekday(*d)))
    }
}

/// A one-off entry in the day planner.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub day: NaiveDate,
    pub time: TimeRange,
    pub title: String,
    pub details: String,
    pub color: String,
    pub id: Option<i64>,
}

impl Event {
    pub fn new(day: NaiveDate, time: TimeRange, title: &str) -> Self {
        Self { day, time, title: title.to_string(), details: String::new(), color: "#8a8f98".into(), id: None }
    }

    pub fn validate(&self) -> Checked {
        if self.title.trim().is_empty() {
            return err("Give the event a title.");
        }
        Ok(())
    }

    pub fn is_milestone(&self) -> bool {
        self.time.duration() == MILESTONE_MINUTES
    }

    pub fn is_all_day(&self) -> bool {
        self.time.duration() == MINUTES_PER_DAY
    }
}

/// The stored span of an all-day event: the whole day, midnight to midnight.
pub fn all_day_time() -> TimeRange {
    TimeRange { start: 0, end: MINUTES_PER_DAY }
}

/// The stored span of a milestone at `minute`: no duration, so the shortest one.
pub fn milestone_time(minute: i32) -> TimeRange {
    let start = minute.min(MINUTES_PER_DAY - MILESTONE_MINUTES);
    TimeRange { start, end: start + MILESTONE_MINUTES }
}

/// A milestone that repeats on chosen weekdays, such as "Wake up" at 06:00.
#[derive(Clone, Debug, PartialEq)]
pub struct Routine {
    pub title: String,
    pub minute: i32,
    pub weekdays: Vec<u8>,
    pub color: String,
    pub id: Option<i64>,
}

impl Routine {
    pub fn new(title: &str, minute: i32) -> Self {
        Self { title: title.to_string(), minute, weekdays: (0..7).collect(), color: "#f5a524".into(), id: None }
    }

    pub fn validate(&self) -> Checked {
        if self.title.trim().is_empty() {
            return err("Give the routine a title.");
        }
        if !(0..MINUTES_PER_DAY).contains(&self.minute) {
            return err("The time must fall within a single day.");
        }
        if self.weekdays.is_empty() || self.weekdays.iter().any(|d| *d > 6) {
            return err("Pick at least one day of the week.");
        }
        Ok(())
    }

    pub fn occurs_on(&self, day: NaiveDate) -> bool {
        self.weekdays.contains(&weekday(day))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TaskKind {
    Task,
    Homework,
    Assignment,
    Exam,
    Reading,
}

impl TaskKind {
    pub const ALL: [TaskKind; 5] = [Self::Task, Self::Homework, Self::Assignment, Self::Exam, Self::Reading];

    /// The stored name (as in the database and sync records).
    pub fn code(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Homework => "homework",
            Self::Assignment => "assignment",
            Self::Exam => "exam",
            Self::Reading => "reading",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.code() == code)
    }
}

/// When a task is due, relative to today. Declaration order is display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DueBucket {
    Overdue,
    Today,
    Upcoming,
    Later,
    Undated,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub title: String,
    pub kind: TaskKind,
    pub course_id: Option<i64>,
    pub due: Option<NaiveDate>,
    pub done: bool,
    pub details: String,
    pub id: Option<i64>,
    /// Set when the task was imported from a school register.
    pub external_id: Option<String>,
}

impl Task {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            kind: TaskKind::Task,
            course_id: None,
            due: None,
            done: false,
            details: String::new(),
            id: None,
            external_id: None,
        }
    }

    pub fn validate(&self) -> Checked {
        if self.title.trim().is_empty() {
            return err("Give the task a title.");
        }
        Ok(())
    }

    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.done && self.due.is_some_and(|d| d < today)
    }

    pub fn bucket(&self, today: NaiveDate) -> DueBucket {
        match (self.done, self.due) {
            (true, _) => DueBucket::Done,
            (false, None) => DueBucket::Undated,
            (false, Some(d)) if d < today => DueBucket::Overdue,
            (false, Some(d)) if d == today => DueBucket::Today,
            (false, Some(d)) if (d - today).num_days() <= UPCOMING_DAYS => DueBucket::Upcoming,
            _ => DueBucket::Later,
        }
    }
}

// ---- pictures in notes ----------------------------------------------------------------------------------------------

pub const IMAGE_SCHEME: &str = "quire-image:";
/// A picture travels to other devices as one sync record, and a record must stay well under the cloud's 1 MiB per
/// document once encoded.
pub const MAX_IMAGE_BYTES: usize = 700_000;
pub const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid pattern"))
}

fn image_ref() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"!\[([^\]\n]*)\]\(quire-image:([0-9a-f]{8,64})\)")
}

/// A picture shown in notes, referenced from Markdown as `![alt](quire-image:<uid>)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteImage {
    pub uid: String,
    pub mime: String,
    pub data: Vec<u8>,
    pub id: Option<i64>,
}

impl NoteImage {
    pub fn check(&self) -> Checked {
        if !IMAGE_TYPES.contains(&self.mime.as_str()) {
            return err("Notes can show PNG, JPEG, GIF and WebP pictures.");
        }
        if self.data.is_empty() {
            return err("That picture is empty.");
        }
        if self.data.len() > MAX_IMAGE_BYTES {
            return err("That picture is too big to keep in a note (over 700 KB).");
        }
        Ok(())
    }
}

pub fn image_markdown(uid: &str, alt: &str) -> String {
    let alt = alt.replace('[', "(").replace(']', ")");
    let alt = alt.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("![{alt}]({IMAGE_SCHEME}{uid})")
}

/// The pictures a note shows, in order (each once).
pub fn image_uids(body: &str) -> Vec<String> {
    let mut seen = Vec::<String>::new();
    for caps in image_ref().captures_iter(body).flatten() {
        let uid = caps[2].to_string();
        if !seen.contains(&uid) {
            seen.push(uid);
        }
    }
    seen
}

fn without_images(text: &str) -> String {
    image_ref().replace_all(text, "$1").into_owned()
}

fn strip_start(text: &str, ch: char) -> &str {
    text.trim_start_matches(ch)
}

/// Python's `str.splitlines()` for the line ends Quire's text uses.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).enumerate().filter_map(move |(i, l)| {
        // splitlines() drops the empty piece after a final newline.
        if i > 0 && l.is_empty() && text.ends_with('\n') && text.split('\n').count() == i + 1 { None } else { Some(l) }
    })
}

/// A note's title is its first non-empty line, minus Markdown heading marks (a picture counts by its description).
pub fn derive_note_title(body: &str) -> String {
    for line in lines(body) {
        let plain = without_images(line);
        let text = strip_start(plain.trim(), '#').trim();
        if !text.is_empty() {
            return text.chars().take(120).collect();
        }
    }
    "Untitled".into()
}

/// The start of a note's text after its title, as plain words: for the notes list.
pub fn note_snippet(body: &str, length: usize) -> String {
    static HEADINGS: OnceLock<Regex> = OnceLock::new();
    static MARKS: OnceLock<Regex> = OnceLock::new();
    static LINKS: OnceLock<Regex> = OnceLock::new();
    static EMPHASIS: OnceLock<Regex> = OnceLock::new();
    let plain = without_images(body);
    let kept: Vec<&str> = lines(&plain).map(str::trim).filter(|l| !l.is_empty()).collect();
    let text = kept.iter().skip(1).copied().collect::<Vec<_>>().join(" ");
    let text = regex(&HEADINGS, r"^#{1,6}\s+|(?<=\s)#{1,6}\s+").replace_all(&text, "").into_owned();
    let text = regex(&MARKS, r"[-*+]\s+\[[ xX]\]\s+|(?:^|(?<=\s))[-*+>]\s+|\*\*|__|`|~~").replace_all(&text, "").into_owned();
    let text = regex(&LINKS, r"\[([^\]]+)\]\([^)]+\)").replace_all(&text, "$1").into_owned();
    let text = regex(&EMPHASIS, r"(?<![\w*])[*_](?=\S)(.+?)(?<=\S)[*_](?![\w*])").replace_all(&text, "$1").into_owned();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= length {
        return text;
    }
    let cut: String = text.chars().take(length - 1).collect();
    format!("{}…", cut.trim_end())
}

pub const TOPIC_LENGTH: usize = 60;

/// Topics group notes within a course; compare them without stray whitespace.
pub fn normalize_topic(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(TOPIC_LENGTH).collect()
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Note {
    pub body: String,
    pub course_id: Option<i64>,
    pub pinned: bool,
    pub updated: Option<NaiveDateTime>,
    pub id: Option<i64>,
    /// e.g. "Cell respiration" within Biology; "" means ungrouped.
    pub topic: String,
    /// A note is filed in a class (`course_id`) or a notebook.
    pub notebook_id: Option<i64>,
}

impl Note {
    pub fn title(&self) -> String {
        derive_note_title(&self.body)
    }
}

/// A group of notes that isn't a class: "Ideas", "Trips", a project.
#[derive(Clone, Debug, PartialEq)]
pub struct Notebook {
    pub name: String,
    pub color: String,
    pub id: Option<i64>,
}

impl Notebook {
    pub fn validate(&self) -> Checked {
        if self.name.trim().is_empty() {
            return err("Give the notebook a name.");
        }
        Ok(())
    }
}

/// A mark published by a teacher on the school register.
#[derive(Clone, Debug, PartialEq)]
pub struct Grade {
    pub external_id: String,
    pub subject: String,
    pub day: NaiveDate,
    /// As the school shows it, e.g. "7½", "8-", "ass".
    pub display: String,
    /// The numeric value, when the mark has one.
    pub value: Option<f64>,
    /// e.g. "Scritto", "Orale".
    pub component: String,
    pub period: String,
    pub notes: String,
    pub cancelled: bool,
    pub course_id: Option<i64>,
}

impl Grade {
    pub fn counts(&self) -> bool {
        self.value.is_some() && !self.cancelled
    }
}

/// On the Italian 1-10 scale Classeviva uses, below 6 is a fail.
pub const PASS_MARK: f64 = 6.0;

/// Mean of the marks that count; None when there are none.
pub fn average(grades: &[Grade]) -> Option<f64> {
    let values: Vec<f64> = grades.iter().filter(|g| g.counts()).filter_map(|g| g.value).collect();
    if values.is_empty() {
        return None;
    }
    Some(round2(values.iter().sum::<f64>() / values.len() as f64))
}

/// Python's `round(x, 2)`: rounded from the float's exact value, ties to even. Rust's formatting does exactly that.
pub fn round2(x: f64) -> f64 {
    format!("{x:.2}").parse().unwrap_or(x)
}

/// One lesson as recorded on the school register, with its topic.
#[derive(Clone, Debug, PartialEq)]
pub struct Lesson {
    pub external_id: String,
    pub day: NaiveDate,
    pub subject: String,
    pub topic: String,
    pub teacher: String,
    /// Position in the school day (1st hour, 2nd hour...).
    pub hour: i32,
    pub course_id: Option<i64>,
}

/// A subject as the school register names it; courses can be linked to one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Subject {
    pub external_id: String,
    pub name: String,
    pub teachers: Vec<String>,
}

pub const PAY_MODES: [&str; 2] = ["hourly", "monthly"];
pub const TAX_MODELS: [&str; 2] = ["italy", "flat"];

/// Somewhere the student works shifts.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub name: String,
    pub color: String,
    /// Gross, in the user's currency; None if not tracked.
    pub hourly_rate: Option<f64>,
    pub id: Option<i64>,
    /// Every weekly pattern the job has had, including ended ones (kept for history).
    pub schedule: Vec<ShiftPattern>,
    /// Share of gross pay withheld for tax and contributions, in percent; used by the "flat" tax model.
    pub deductions: f64,
    /// "hourly", or "monthly" (a fixed gross amount per month).
    pub pay_mode: String,
    /// Gross, per mensilità.
    pub monthly_pay: Option<f64>,
    pub mensilities: u8,
    pub contract_start: Option<NaiveDate>,
    pub contract_end: Option<NaiveDate>,
    /// "flat" percentage, or "italy": INPS + IRPEF.
    pub tax_model: String,
    /// "italy": employee contributions, percent.
    pub inps: f64,
    /// "italy": regional + municipal surtax, percent.
    pub addizionali: f64,
    pub fixed_term: bool,
    /// "italy": add the low-income cuneo fiscale bonus.
    pub cuneo: bool,
}

impl Job {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            color: "#0090ff".into(),
            hourly_rate: None,
            id: None,
            schedule: Vec::new(),
            deductions: 0.0,
            pay_mode: "hourly".into(),
            monthly_pay: None,
            mensilities: 13,
            contract_start: None,
            contract_end: None,
            tax_model: "flat".into(),
            inps: 9.19,
            addizionali: 0.0,
            fixed_term: true,
            cuneo: false,
        }
    }

    pub fn validate(&self) -> Checked {
        if self.name.trim().is_empty() {
            return err("Give the job a name.");
        }
        if self.hourly_rate.is_some_and(|r| r < 0.0) {
            return err("The hourly rate can't be negative.");
        }
        if !(0.0..100.0).contains(&self.deductions) {
            return err("Tax and deductions must be between 0% and 100%.");
        }
        if !PAY_MODES.contains(&self.pay_mode.as_str()) || !TAX_MODELS.contains(&self.tax_model.as_str()) {
            return err("Pick how the job is paid and taxed.");
        }
        if self.monthly_pay.is_some_and(|p| p < 0.0) {
            return err("The monthly pay can't be negative.");
        }
        if ![12, 13, 14].contains(&self.mensilities) {
            return err("A job pays 12, 13 or 14 monthly payments a year.");
        }
        if !(0.0..100.0).contains(&self.inps) || !(0.0..100.0).contains(&self.addizionali) {
            return err("Contributions and surtax must be between 0% and 100%.");
        }
        if let (Some(start), Some(end)) = (self.contract_start, self.contract_end) {
            if end < start {
                return err("The contract can't end before it starts.");
            }
        }
        self.schedule.iter().try_for_each(ShiftPattern::validate)
    }

    /// Patterns that still apply from `today` on (the ones you'd edit).
    pub fn active_schedule(&self, today: NaiveDate) -> Vec<&ShiftPattern> {
        let mut active: Vec<&ShiftPattern> = self.schedule.iter().filter(|p| p.until.is_none_or(|u| u >= today)).collect();
        active.sort_by_key(|p| (p.weekday, p.start));
        active
    }
}

pub const MAX_SHIFT_MINUTES: i32 = 16 * 60;

/// Take-home pay once `deductions` percent is withheld.
pub fn net_pay(gross: Option<f64>, deductions: f64) -> Option<f64> {
    gross.map(|g| round2(g * (1.0 - deductions / 100.0)))
}

/// One work shift. It may run past midnight into the next day.
#[derive(Clone, Debug, PartialEq)]
pub struct Shift {
    pub job_id: i64,
    pub day: NaiveDate,
    /// Minutes after midnight on `day`.
    pub start: i32,
    /// Minutes, including the break.
    pub duration: i32,
    pub break_minutes: i32,
    pub notes: String,
    pub id: Option<i64>,
    /// An occurrence of the job's weekly schedule, not stored.
    pub recurring: bool,
}

impl Shift {
    pub fn new(job_id: i64, day: NaiveDate, start: i32, duration: i32) -> Self {
        Self { job_id, day, start, duration, break_minutes: 0, notes: String::new(), id: None, recurring: false }
    }

    /// Build a shift from clock times; an end at or before the start means the next day.
    pub fn between(job_id: i64, day: NaiveDate, start: i32, end: i32) -> Self {
        let duration = if end > start { end - start } else { end + MINUTES_PER_DAY - start };
        Self::new(job_id, day, start, duration)
    }

    pub fn validate(&self) -> Checked {
        check_shift_times(self.start, self.duration, self.break_minutes)
    }

    /// Minutes after midnight on `day`; above 1440 when the shift ends the next day.
    pub fn end(&self) -> i32 {
        self.start + self.duration
    }

    pub fn ends_next_day(&self) -> bool {
        self.end() > MINUTES_PER_DAY
    }

    pub fn paid_minutes(&self) -> i32 {
        self.duration - self.break_minutes
    }

    pub fn pay(&self, hourly_rate: Option<f64>) -> Option<f64> {
        hourly_rate.map(|r| round2(f64::from(self.paid_minutes()) / 60.0 * r))
    }

    /// The parts of the shift on each calendar day it touches.
    pub fn segments(&self) -> Vec<(NaiveDate, TimeRange)> {
        if !self.ends_next_day() {
            return vec![(self.day, TimeRange { start: self.start, end: self.end() })];
        }
        vec![
            (self.day, TimeRange { start: self.start, end: MINUTES_PER_DAY }),
            (self.day + Duration::days(1), TimeRange { start: 0, end: self.end() - MINUTES_PER_DAY }),
        ]
    }

    pub fn overlaps(&self, day: NaiveDate, time: TimeRange) -> bool {
        self.segments().iter().any(|(d, seg)| *d == day && seg.start < time.end && time.start < seg.end)
    }
}

fn check_shift_times(start: i32, duration: i32, break_minutes: i32) -> Checked {
    if !(0..MINUTES_PER_DAY).contains(&start) {
        return err("The shift must start within the day.");
    }
    if !(0 < duration && duration <= MAX_SHIFT_MINUTES) {
        return err("A shift must last between a minute and 16 hours.");
    }
    if !(0 <= break_minutes && break_minutes < duration) {
        return err("The break must be shorter than the shift.");
    }
    Ok(())
}

/// A shift that repeats every week, like a class in the school timetable. `since`/`until` bound the weeks it applies
/// to, so changing a schedule doesn't rewrite the hours already worked.
#[derive(Clone, Debug, PartialEq)]
pub struct ShiftPattern {
    pub weekday: u8,
    pub start: i32,
    pub duration: i32,
    pub break_minutes: i32,
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    pub id: Option<i64>,
}

impl ShiftPattern {
    pub fn between(weekday: u8, start: i32, end: i32, break_minutes: i32) -> Self {
        let duration = if end > start { end - start } else { end + MINUTES_PER_DAY - start };
        Self { weekday, start, duration, break_minutes, since: None, until: None, id: None }
    }

    pub fn validate(&self) -> Checked {
        if self.weekday > 6 {
            return err("Weekday must be between Monday and Sunday.");
        }
        check_shift_times(self.start, self.duration, self.break_minutes)?;
        if let (Some(since), Some(until)) = (self.since, self.until) {
            if until < since {
                return err("A repeating shift can't end before it starts.");
            }
        }
        Ok(())
    }

    /// What makes two patterns the same shift.
    pub fn key(&self) -> (u8, i32, i32, i32) {
        (self.weekday, self.start, self.duration, self.break_minutes)
    }

    pub fn occurs_on(&self, day: NaiveDate) -> bool {
        weekday(day) == self.weekday && self.since.is_none_or(|s| day >= s) && self.until.is_none_or(|u| day <= u)
    }

    pub fn shift_on(&self, job_id: i64, day: NaiveDate) -> Shift {
        Shift { break_minutes: self.break_minutes, recurring: true, ..Shift::new(job_id, day, self.start, self.duration) }
    }
}

pub const WORK_DAYS: [u8; 5] = [0, 1, 2, 3, 4];
pub const EVERY_DAY: [u8; 7] = [0, 1, 2, 3, 4, 5, 6];

/// All shifts starting on days `first..=last`: one-off shifts plus weekly-schedule occurrences, minus the occurrences
/// the user skipped (job id, day, start).
pub fn work_shifts(
    jobs: &[Job],
    one_off: &[Shift],
    skipped: &HashSet<(i64, NaiveDate, i32)>,
    first: NaiveDate,
    last: NaiveDate,
) -> Vec<Shift> {
    let mut result: Vec<Shift> = one_off.iter().filter(|s| first <= s.day && s.day <= last).cloned().collect();
    let mut day = first;
    while day <= last {
        for job in jobs {
            let Some(id) = job.id else { continue };
            for pattern in &job.schedule {
                if pattern.occurs_on(day) && !skipped.contains(&(id, day, pattern.start)) {
                    result.push(pattern.shift_on(id, day));
                }
            }
        }
        day += Duration::days(1);
    }
    result.sort_by_key(|s| (s.day, s.start));
    result
}

/// Apply a new weekly schedule from `this_week` (a Monday) on. Unchanged patterns are kept as they are. Patterns that
/// are no longer wanted end the Sunday before `this_week`, or disappear entirely if they started this week. New ones
/// start `this_week`. Patterns that ended earlier stay for history.
pub fn reschedule(current: &[ShiftPattern], wanted: &[ShiftPattern], this_week: NaiveDate) -> Vec<ShiftPattern> {
    let is_active = |p: &ShiftPattern| p.until.is_none_or(|u| u >= this_week);
    let wanted_keys: HashSet<_> = wanted.iter().map(ShiftPattern::key).collect();
    let mut active_keys: HashSet<_> = current.iter().filter(|p| is_active(p)).map(ShiftPattern::key).collect();
    let mut result: Vec<ShiftPattern> = current.iter().filter(|p| !is_active(p)).cloned().collect();
    for pattern in current.iter().filter(|p| is_active(p)) {
        if wanted_keys.contains(&pattern.key()) {
            result.push(pattern.clone());
        } else if pattern.since.is_none_or(|s| s < this_week) {
            result.push(ShiftPattern { until: Some(this_week - Duration::days(1)), ..pattern.clone() });
        }
        // Otherwise it only ever applied from this week on, so it simply goes away.
    }
    for pattern in wanted {
        if active_keys.insert(pattern.key()) {
            result.push(ShiftPattern { since: Some(this_week), until: None, id: None, ..pattern.clone() });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn time_ranges_stay_within_a_day() {
        assert!(TimeRange::new(60, 120).is_ok());
        assert_eq!(TimeRange::new(120, 60).unwrap_err().0, "The end time must be after the start time.");
        assert!(TimeRange::new(0, MINUTES_PER_DAY).is_ok());
        assert!(TimeRange::new(-1, 10).is_err());
    }

    #[test]
    fn task_buckets() {
        let today = d(2026, 3, 10);
        let task = |due: Option<NaiveDate>, done: bool| Task { due, done, ..Task::new("x") };
        assert_eq!(task(None, false).bucket(today), DueBucket::Undated);
        assert_eq!(task(Some(d(2026, 3, 9)), false).bucket(today), DueBucket::Overdue);
        assert_eq!(task(Some(today), false).bucket(today), DueBucket::Today);
        assert_eq!(task(Some(d(2026, 3, 17)), false).bucket(today), DueBucket::Upcoming);
        assert_eq!(task(Some(d(2026, 3, 18)), false).bucket(today), DueBucket::Later);
        assert_eq!(task(Some(d(2026, 1, 1)), true).bucket(today), DueBucket::Done);
        assert!(!task(Some(d(2026, 1, 1)), true).is_overdue(today));
    }

    #[test]
    fn note_titles() {
        for (body, title) in [("# Heading\nbody", "Heading"), ("\n\n  ## Spaced  \n", "Spaced"), ("", "Untitled"), ("###", "Untitled")] {
            assert_eq!(derive_note_title(body), title, "{body:?}");
        }
        assert_eq!(normalize_topic("  Cell   respiration "), "Cell respiration");
    }

    #[test]
    fn note_snippets_are_plain_words_after_the_title() {
        let body = "# Mitochondria\n\nThe **powerhouse** of the cell\n- [ ] see [p. 4](http://x)";
        assert_eq!(note_snippet(body, 90), "The powerhouse of the cell see p. 4");
        assert_eq!(note_snippet("Only a title", 90), "");
        assert_eq!(note_snippet("T\n*Nature vs. nurture* and _this_", 90), "Nature vs. nurture and this");
    }

    #[test]
    fn pictures_in_notes() {
        assert_eq!(image_markdown("ab12cd34", "ER [v2]"), "![ER (v2)](quire-image:ab12cd34)");
        let body = "# Title\n![a](quire-image:ab12cd34ab12cd34)\ntext ![b](quire-image:ab12cd34ab12cd34)";
        assert_eq!(image_uids(body), vec!["ab12cd34ab12cd34"]);
        let body = "![](quire-image:ab12cd34ab12cd34)\n# Schema\n![Diagram](quire-image:ab12cd34)\nNotes";
        assert_eq!(derive_note_title(body), "Schema");
        assert_eq!(note_snippet(body, 90), "Diagram Notes");
    }

    #[test]
    fn overnight_shifts_split_at_midnight() {
        let shift = Shift::between(1, d(2026, 3, 10), 22 * 60, 6 * 60);
        assert_eq!(shift.duration, 8 * 60);
        assert!(shift.ends_next_day());
        let parts = shift.segments();
        assert_eq!(parts[1], (d(2026, 3, 11), TimeRange { start: 0, end: 360 }));
        assert_eq!(Shift { break_minutes: 30, ..shift.clone() }.pay(Some(10.0)), Some(75.0));
    }

    #[test]
    fn rescheduling_keeps_history() {
        let monday = d(2026, 3, 9);
        let old = ShiftPattern { since: Some(d(2026, 1, 5)), ..ShiftPattern::between(0, 540, 780, 0) };
        let kept = ShiftPattern::between(2, 540, 780, 0);
        let new = ShiftPattern::between(4, 600, 840, 30);
        let result = reschedule(&[old.clone(), kept.clone()], &[kept.clone(), new.clone()], monday);
        assert!(result.contains(&ShiftPattern { until: Some(d(2026, 3, 8)), ..old }));
        assert!(result.contains(&kept));
        assert!(result.iter().any(|p| p.key() == new.key() && p.since == Some(monday)));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn rounding_matches_python() {
        assert_eq!(round2(7.125), 7.12);
        assert_eq!(round2(2.675), 2.67); // the float is just under, as in Python
        assert_eq!(round2(6.666), 6.67);
        assert_eq!(round2(0.125), 0.12);
        assert_eq!(round2(1.005), 1.0);
        assert_eq!(round2(-2.675), -2.67);
    }
}
