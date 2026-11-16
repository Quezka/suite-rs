//! The school register account and mirroring it into Quire (port of `services/school.py`): connect, sync, link
//! courses to subjects. `fetch()` only talks to the register (worker thread); `apply()` writes locally (UI thread).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use chrono::{Duration, NaiveDateTime};

use super::bus::{ChangeBus, Topic};
use super::errors::{AppError, AppResult};
use super::ports::{Clock, CourseRepository, CourseLookup, CredentialStore, Credentials, KeyValueStore, RegisterAccount,
                   RegisterError, RegisterSnapshot, RemoteAssignment, SchoolLinks, SchoolRecordRepository, SchoolRegister,
                   TaskRepository};
use crate::domain::model::{Course, Grade, Lesson, Subject, Task, COURSE_COLORS};
use crate::domain::register::Notice;

pub const ASSIGNMENTS_PAST_DAYS: i64 = 30;
pub const ASSIGNMENTS_FUTURE_DAYS: i64 = 120;
pub const LESSON_DAYS: i64 = 21;
pub const TITLE_LENGTH: usize = 90;
pub const PARTS: usize = 10;

/// Registers shout ("MATEMATICA"); store names in sentence case.
pub fn tidy_subject(name: &str) -> String {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if is_upper(&name) { capitalize(&name) } else { name }
}

pub fn tidy_person(name: &str) -> String {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if is_upper(&name) { title_case(&name) } else { name }
}

/// Python's str.isupper(): has cased letters and none lower-case.
fn is_upper(s: &str) -> bool {
    s.chars().any(char::is_alphabetic) && !s.chars().any(char::is_lowercase)
}

/// Python's str.capitalize(): first character upper, the rest lower.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Python's str.title(): upper after any non-letter, lower otherwise.
fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut prev_letter = false;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            if prev_letter { out.extend(ch.to_lowercase()) } else { out.extend(ch.to_uppercase()) }
            prev_letter = true;
        } else {
            out.push(ch);
            prev_letter = false;
        }
    }
    out
}

pub fn assignment_title(item: &RemoteAssignment) -> String {
    let first = item.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
    let title = if !first.is_empty() { first } else {
        let s = tidy_subject(&item.subject_name);
        if s.is_empty() { "School assignment".into() } else { s }
    };
    if title.chars().count() <= TITLE_LENGTH {
        return title;
    }
    let cut: String = title.chars().take(TITLE_LENGTH - 1).collect();
    format!("{}…", cut.trim_end())
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchoolStatus {
    pub register: String,
    pub connected: bool,
    pub username: String,
    pub student_name: String,
    pub last_sync: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncReport {
    pub first_sync: bool,
    pub new_tasks: Vec<Task>,
    pub updated_tasks: Vec<Task>,
    pub removed_tasks: usize,
    pub new_grades: Vec<Grade>,
    pub lessons: usize,
    pub courses_created: usize,
    pub problems: Vec<String>,
    pub new_notices: Vec<String>,
    pub new_absences: usize,
}

impl SyncReport {
    pub fn has_news(&self) -> bool {
        !self.new_tasks.is_empty() || !self.updated_tasks.is_empty() || !self.new_grades.is_empty() || !self.new_notices.is_empty() || self.new_absences > 0
    }
}

fn register_error(e: RegisterError) -> AppError {
    match e {
        RegisterError::Authentication(m) | RegisterError::Failed(m) => AppError::Register(m),
    }
}

pub struct SchoolSyncService {
    pub register: Arc<dyn SchoolRegister>,
    pub credentials: Rc<dyn CredentialStore>,
    pub courses: Rc<dyn CourseRepository>,
    pub lookup: Rc<dyn CourseLookup>,
    pub tasks: Rc<dyn TaskRepository>,
    pub records: Rc<dyn SchoolRecordRepository>,
    pub links: Rc<dyn SchoolLinks>,
    pub settings: Rc<dyn KeyValueStore>,
    pub clock: Rc<dyn Clock>,
    pub bus: Rc<ChangeBus>,
}

impl SchoolSyncService {
    fn source(&self) -> String {
        self.register.name().to_lowercase()
    }

    fn key(&self, name: &str) -> String {
        format!("{}.{name}", self.source())
    }

    fn external(&self, kind: &str, remote: &str) -> String {
        format!("{}:{kind}:{remote}", self.source())
    }

    pub fn status(&self) -> SchoolStatus {
        let username = self.settings.get(&self.key("username")).unwrap_or_default();
        SchoolStatus {
            register: self.register.name(), connected: !username.is_empty(), username,
            student_name: self.settings.get(&self.key("student")).unwrap_or_default(),
            last_sync: self.settings.get(&self.key("last_sync")).and_then(|t| NaiveDateTime::parse_from_str(&t, "%Y-%m-%dT%H:%M:%S").ok()),
        }
    }

    /// Check the login with the register, then remember it. Network: worker thread is better, but it's quick.
    pub fn connect(&self, username: &str, password: &str) -> AppResult<RegisterAccount> {
        let c = Credentials { username: username.trim().into(), password: password.into() };
        let account = self.register.login(&c).map_err(register_error)?;
        self.credentials.save(&c)?;
        self.settings.set(&self.key("username"), Some(&c.username))?;
        self.settings.set(&self.key("student"), Some(&account.student_name))?;
        self.bus.publish(Topic::School);
        Ok(account)
    }

    /// The saved register login, for the phone's setup code.
    pub fn login_for_phone(&self) -> Option<Credentials> {
        self.credentials.load()
    }

    /// Forget the account. Imported tasks, grades and lessons stay.
    pub fn disconnect(&self) -> AppResult {
        self.credentials.clear()?;
        for n in ["username", "student", "last_sync"] {
            self.settings.set(&self.key(n), None)?;
        }
        self.bus.publish(Topic::School);
        Ok(())
    }

    /// Everything from the register, part by part (one broken part doesn't stop the rest). Network only.
    pub fn fetch(register: &dyn SchoolRegister, credentials: Option<Credentials>, today: chrono::NaiveDate) -> AppResult<RegisterSnapshot> {
        let Some(c) = credentials else {
            return Err(AppError::Register(format!("Connect your {} account first.", register.name())));
        };
        let assignment_window = (today - Duration::days(ASSIGNMENTS_PAST_DAYS), today + Duration::days(ASSIGNMENTS_FUTURE_DAYS));
        let lesson_window = (today - Duration::days(LESSON_DAYS), today);
        let account = register.login(&c).map_err(register_error)?;
        let mut problems: Vec<String> = Vec::new();
        let mut auth: Option<String> = None;
        macro_rules! part {
            ($label:expr, $call:expr) => {
                match $call {
                    Ok(v) => Some(v),
                    Err(RegisterError::Authentication(m)) => { auth.get_or_insert(m); None }
                    Err(RegisterError::Failed(m)) => { problems.push(format!("{}: {}", $label, m)); None }
                }
            };
        }
        let snapshot = RegisterSnapshot {
            account,
            subjects: part!("Subjects", register.subjects()),
            assignments: part!("Agenda (homework and tests)", register.assignments(assignment_window.0, assignment_window.1)),
            homework: part!("Homework", register.homework()),
            grades: part!("Grades", register.grades()),
            lessons: part!("Lesson topics", register.lessons(lesson_window.0, lesson_window.1)),
            assignment_window: Some(assignment_window),
            lesson_window: Some(lesson_window),
            absences: part!("Absences", register.absences()),
            notices: part!("Noticeboard", register.notices()),
            books: part!("Textbooks", register.books()),
            documents: part!("Documents", register.documents()),
            school_days: part!("School calendar", register.school_days()),
            problems: Vec::new(),
        };
        if let Some(m) = auth {
            return Err(AppError::Register(m));
        }
        if problems.len() == PARTS {
            return Err(AppError::Register(format!("Couldn't sync anything from {}. {}", register.name(), problems.join(" "))));
        }
        Ok(RegisterSnapshot { problems, ..snapshot })
    }

    pub fn sync(&self) -> AppResult<SyncReport> {
        let snap = Self::fetch(&*self.register, self.credentials.load(), self.clock.today())?;
        self.apply(&snap)
    }

    pub fn apply(&self, snap: &RegisterSnapshot) -> AppResult<SyncReport> {
        let first_sync = self.settings.get(&self.key("last_sync")).is_none();
        let (subject_ids, subject_names, created) = self.sync_courses(snap)?;
        let course_for = |id: &Option<String>, name: &str| -> Option<i64> {
            id.as_ref().and_then(|i| subject_ids.get(i).copied()).or_else(|| subject_names.get(&tidy_subject(name).to_lowercase()).copied())
        };
        let mut report = SyncReport { first_sync, courses_created: created, problems: snap.problems.clone(), ..Default::default() };
        for (items, feed, window) in [(&snap.assignments, "agenda", snap.assignment_window), (&snap.homework, "homework", None)] {
            if let Some(items) = items {
                let (added, changed, dropped) = self.sync_assignments(items, feed, window, &course_for)?;
                report.new_tasks.extend(added);
                report.updated_tasks.extend(changed);
                report.removed_tasks += dropped;
            }
        }
        if let Some(remote) = &snap.grades {
            let known: HashSet<String> = self.records.grades()?.into_iter().map(|g| g.external_id).collect();
            let grades: Vec<Grade> = remote.iter().map(|g| Grade {
                external_id: self.external("grade", &g.id), subject: tidy_subject(&g.subject_name), day: g.day, display: g.display.clone(),
                value: g.value, component: g.component.clone(), period: g.period.clone(), notes: g.notes.clone(), cancelled: g.cancelled,
                course_id: course_for(&g.subject_id, &g.subject_name),
            }).collect();
            self.records.replace_grades(&grades)?;
            report.new_grades = grades.into_iter().filter(|g| !known.contains(&g.external_id)).collect();
        }
        if let Some(remote) = &snap.lessons {
            let lessons: Vec<Lesson> = remote.iter().map(|l| Lesson {
                external_id: self.external("lesson", &l.id), day: l.day, subject: tidy_subject(&l.subject_name), topic: l.topic.clone(),
                teacher: tidy_person(&l.teacher), hour: l.hour, course_id: course_for(&l.subject_id, &l.subject_name),
            }).collect();
            if let Some((first, last)) = snap.lesson_window {
                self.records.replace_lessons(first, last, &lessons)?;
            }
            report.lessons = lessons.len();
        }
        let (notices, absences) = self.apply_register_items(snap)?;
        report.new_notices = notices;
        report.new_absences = absences;
        self.settings.set(&self.key("student"), Some(&snap.account.student_name))?;
        self.settings.set(&self.key("last_sync"), Some(&self.clock.now().format("%Y-%m-%dT%H:%M:%S").to_string()))?;
        if created > 0 {
            self.bus.publish(Topic::Courses);
        }
        self.bus.publish(Topic::Tasks);
        self.bus.publish(Topic::School);
        Ok(report)
    }

    /// Absences, noticeboard, textbooks, documents and the calendar: stored as they are.
    fn apply_register_items(&self, snap: &RegisterSnapshot) -> AppResult<(Vec<String>, usize)> {
        let (mut new_notices, mut new_absences) = (Vec::new(), 0);
        if let Some(remote) = &snap.absences {
            let local: HashMap<String, Option<i32>> = self.records.absences()?.into_iter().map(|a| (a.external_id, a.own_hour)).collect();
            let absences: Vec<_> = remote.iter().map(|a| {
                let key = self.external("absence", &a.external_id);
                // keep an hour you entered; the register never sends one back
                let own = local.get(&key).copied().flatten();
                crate::domain::register::Absence { external_id: key, own_hour: own, ..a.clone() }
            }).collect();
            new_absences = absences.iter().filter(|a| !local.contains_key(&a.external_id)).count();
            self.records.replace_absences(&absences)?;
        }
        if let Some(remote) = &snap.notices {
            let local: HashMap<String, bool> = self.records.notices()?.into_iter().map(|n| (n.external_id, n.read)).collect();
            let notices: Vec<Notice> = remote.iter().map(|n| {
                let key = self.external("notice", &n.external_id);
                // read here but not yet told to the register: stay read
                let read = n.read || local.get(&key).copied().unwrap_or(false);
                Notice { external_id: key, read, ..n.clone() }
            }).collect();
            new_notices = notices.iter().filter(|n| !local.contains_key(&n.external_id)).map(|n| n.title.clone()).collect();
            self.records.replace_notices(&notices)?;
        }
        if let Some(b) = &snap.books {
            self.records.replace_books(b)?;
        }
        if let Some(d) = &snap.documents {
            self.records.replace_documents(d)?;
        }
        if let Some(d) = &snap.school_days {
            self.records.replace_school_days(d)?;
        }
        Ok((new_notices, new_absences))
    }

    /// Opens a notice on the register (network); then call `mark_notice_read` on the UI thread.
    pub fn notice_text(register: &dyn SchoolRegister, credentials: Option<Credentials>, notice: &Notice) -> AppResult<String> {
        let c = credentials.ok_or_else(|| AppError::Register(format!("Connect your {} account first.", register.name())))?;
        register.login(&c).map_err(register_error)?;
        register.open_notice(notice).map_err(register_error)
    }

    pub fn mark_notice_read(&self, id: &str) -> AppResult {
        self.records.set_notice_read(id)?;
        self.bus.publish(Topic::School);
        Ok(())
    }

    /// Match register subjects to local courses, creating any missing.
    fn sync_courses(&self, snap: &RegisterSnapshot) -> AppResult<(HashMap<String, i64>, HashMap<String, i64>, usize)> {
        if let Some(subjects) = snap.subjects.as_ref().filter(|s| !s.is_empty()) {
            self.records.replace_subjects(&subjects.iter().map(|x| Subject {
                external_id: self.external("subject", &x.id), name: tidy_subject(&x.name),
                teachers: x.teachers.iter().filter(|t| !t.is_empty()).map(|t| tidy_person(t)).collect(),
            }).collect::<Vec<_>>())?;
        }
        let mut courses = self.courses.list()?;
        let mut used: HashSet<String> = courses.iter().map(|c| c.color.clone()).collect();
        let mut ids = HashMap::new();
        let mut created = 0;
        for x in snap.subjects.iter().flatten() {
            let external = self.external("subject", &x.id);
            let name = tidy_subject(&x.name);
            let teacher = x.teachers.iter().map(|t| tidy_person(t)).collect::<Vec<_>>().join(", ");
            let found = courses.iter().position(|c| c.external_id.as_deref() == Some(external.as_str()))
                .or_else(|| courses.iter().position(|c| c.name.to_lowercase() == name.to_lowercase()));
            let id = match found {
                None => {
                    let color = COURSE_COLORS.iter().find(|c| !used.contains(**c)).copied()
                        .unwrap_or(COURSE_COLORS[used.len() % COURSE_COLORS.len()]).to_string();
                    used.insert(color.clone());
                    let mut course = Course { teacher, color, external_id: Some(external), ..Course::named(&name) };
                    course.id = Some(self.courses.add(&course)?);
                    courses.push(course.clone());
                    created += 1;
                    course.id.unwrap()
                }
                Some(i) => {
                    let course = &mut courses[i];
                    if course.external_id.as_deref() != Some(external.as_str()) || (!teacher.is_empty() && course.teacher.is_empty()) {
                        course.external_id = Some(external);
                        if course.teacher.is_empty() {
                            course.teacher = teacher;
                        }
                        self.courses.update(course)?;
                    }
                    course.id.unwrap_or_default()
                }
            };
            ids.insert(x.id.clone(), id);
        }
        let names = courses.iter().map(|c| (c.name.to_lowercase(), c.id.unwrap_or_default())).collect();
        Ok((ids, names, created))
    }

    /// Mirror one feed of register work into tasks; with a `window` the feed is complete for those dates, so undone tasks
    /// that vanished were deleted by the teacher and go here too.
    fn sync_assignments(&self, items: &[RemoteAssignment], feed: &str, window: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
                        course_for: &dyn Fn(&Option<String>, &str) -> Option<i64>) -> AppResult<(Vec<Task>, Vec<Task>, usize)> {
        let prefix = self.external(feed, "");
        let existing: HashMap<String, Task> = self.links.tasks_by_external_prefix(&prefix)?.into_iter()
            .filter_map(|t| t.external_id.clone().map(|e| (e, t))).collect();
        let (mut seen, mut new, mut updated) = (HashSet::new(), Vec::new(), Vec::new());
        for item in items {
            let external = format!("{prefix}{}", item.id);
            seen.insert(external.clone());
            let mut details = item.text.trim().to_string();
            if !item.author.is_empty() {
                details.push_str(&format!("\n\n— {}", tidy_person(&item.author)));
            }
            let (title, course) = (assignment_title(item), course_for(&item.subject_id, &item.subject_name));
            match existing.get(&external) {
                None => {
                    let mut t = Task { title, kind: item.kind, due: Some(item.day), course_id: course, details, done: item.done,
                                       external_id: Some(external), ..Task::new("") };
                    t.id = Some(self.tasks.add(&t)?);
                    new.push(t);
                }
                Some(t) => {
                    let differs = t.title != title || t.kind != item.kind || t.due != Some(item.day) || t.course_id != course || t.details != details;
                    if differs || (item.done && !t.done) {
                        // keeps the student's own tick; a "done" on the register is carried over
                        let t = Task { title, kind: item.kind, due: Some(item.day), course_id: course, details, done: t.done || item.done, ..t.clone() };
                        self.tasks.update(&t)?;
                        updated.push(t);
                    }
                }
            }
        }
        let mut removed = 0;
        if let Some((first, last)) = window {
            for (external, t) in &existing {
                // teacher deleted it: drop it, unless the student already ticked it off
                if !seen.contains(external) && !t.done && t.due.is_some_and(|d| first <= d && d <= last) {
                    self.tasks.delete(t.id.unwrap_or_default())?;
                    removed += 1;
                }
            }
        }
        Ok((new, updated, removed))
    }

    /// Make `course_id` the course a register subject syncs into (None unlinks). A course the sync created for that subject
    /// is folded into this one; one with class times of its own is only unlinked.
    pub fn link_course(&self, course_id: i64, subject_id: Option<&str>) -> AppResult {
        let mut course = self.lookup.get(course_id)?.ok_or_else(|| AppError::NotFound(format!("Course {course_id} does not exist.")))?;
        if course.external_id.as_deref() == subject_id {
            return Ok(());
        }
        if let Some(sid) = subject_id {
            let subject = self.records.subjects()?.into_iter().find(|s| s.external_id == sid)
                .ok_or_else(|| AppError::NotFound("That subject isn't in the last sync. Sync again and retry.".into()))?;
            if let Some(mut previous) = self.courses.list()?.into_iter().find(|c| c.external_id.as_deref() == Some(sid) && c.id != Some(course_id)) {
                if !previous.slots.is_empty() {
                    previous.external_id = None;
                    self.courses.update(&previous)?;
                } else {
                    self.links.merge_courses(previous.id.unwrap_or_default(), course_id)?;
                }
            }
            if course.teacher.is_empty() && !subject.teachers.is_empty() {
                course.teacher = subject.teachers.join(", ");
            }
        }
        course.external_id = subject_id.map(str::to_string);
        self.courses.update(&course)?;
        self.bus.publish(Topic::Courses);
        self.bus.publish(Topic::Tasks);
        self.bus.publish(Topic::School);
        Ok(())
    }
}
