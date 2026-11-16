//! Mirroring the school register into Quire, with a fake register (as the Python app's tests use FakeRegister).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, NaiveDateTime};
use quire::application::bus::ChangeBus;
use quire::application::errors::AppResult;
use quire::application::ports::*;
use quire::application::school::{tidy_person, tidy_subject, SchoolSyncService};
use quire::domain::model::TaskKind;
use quire::domain::register::{Absence, AbsenceKind, Book, Notice, SchoolDocument};
use quire::infrastructure::repositories::*;
use quire::infrastructure::sqlite::Database;

struct Fixed(Cell<NaiveDateTime>);
impl Clock for Fixed {
    fn now(&self) -> NaiveDateTime {
        self.0.get()
    }
}

#[derive(Default)]
struct Keyring(RefCell<Option<Credentials>>);
impl CredentialStore for Keyring {
    fn load(&self) -> Option<Credentials> { self.0.borrow().clone() }
    fn save(&self, c: &Credentials) -> AppResult { *self.0.borrow_mut() = Some(c.clone()); Ok(()) }
    fn clear(&self) -> AppResult { *self.0.borrow_mut() = None; Ok(()) }
}

fn day(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

#[derive(Default)]
struct FakeRegister {
    agenda: Mutex<Vec<RemoteAssignment>>,
    grades_fail: bool,
}

impl SchoolRegister for FakeRegister {
    fn name(&self) -> String { "Classeviva".into() }
    fn login(&self, c: &Credentials) -> RegisterResult<RegisterAccount> {
        if c.password == "pw" { Ok(RegisterAccount { student_name: "ANNA ROSSI".into() }) } else { Err(RegisterError::Authentication("Wrong username or password.".into())) }
    }
    fn subjects(&self) -> RegisterResult<Vec<RemoteSubject>> {
        Ok(vec![RemoteSubject { id: "1".into(), name: "MATEMATICA".into(), teachers: vec!["OKAFOR JANE".into()] }])
    }
    fn assignments(&self, _f: NaiveDate, _l: NaiveDate) -> RegisterResult<Vec<RemoteAssignment>> { Ok(self.agenda.lock().unwrap().clone()) }
    fn homework(&self) -> RegisterResult<Vec<RemoteAssignment>> { Ok(vec![]) }
    fn grades(&self) -> RegisterResult<Vec<RemoteGrade>> {
        if self.grades_fail { return Err(RegisterError::Failed("timeout".into())); }
        Ok(vec![RemoteGrade { id: "g1".into(), day: day("2026-09-20"), subject_id: Some("1".into()), subject_name: "MATEMATICA".into(),
            display: "7½".into(), value: Some(7.5), component: "Scritto".into(), period: String::new(), notes: String::new(), cancelled: false }])
    }
    fn lessons(&self, _f: NaiveDate, _l: NaiveDate) -> RegisterResult<Vec<RemoteLesson>> { Ok(vec![]) }
    fn absences(&self) -> RegisterResult<Vec<Absence>> {
        Ok(vec![Absence { external_id: "a1".into(), day: day("2026-09-21"), kind: AbsenceKind::Late, hour: None, justified: false,
                          reason: String::new(), hours: 0, own_hour: None }])
    }
    fn notices(&self) -> RegisterResult<Vec<Notice>> {
        Ok(vec![Notice { external_id: "n1".into(), code: "CF".into(), pub_id: "9".into(), title: "Gita".into(), category: String::new(),
                         published: day("2026-09-22"), valid_until: None, read: false, attachments: vec![] }])
    }
    fn open_notice(&self, _n: &Notice) -> RegisterResult<String> { Ok("Testo".into()) }
    fn notice_attachment(&self, _n: &Notice, _i: i32) -> RegisterResult<Vec<u8>> { Ok(vec![]) }
    fn books(&self) -> RegisterResult<Vec<Book>> { Ok(vec![]) }
    fn documents(&self) -> RegisterResult<Vec<SchoolDocument>> { Ok(vec![]) }
    fn document_file(&self, _d: &SchoolDocument) -> RegisterResult<Vec<u8>> { Ok(vec![]) }
    fn school_days(&self) -> RegisterResult<Vec<NaiveDate>> { Ok(vec![day("2026-09-23")]) }
}

fn agenda(id: &str, text: &str, due: &str) -> RemoteAssignment {
    RemoteAssignment { id: id.into(), day: day(due), kind: TaskKind::Homework, text: text.into(), subject_id: Some("1".into()),
                       subject_name: "MATEMATICA".into(), author: "OKAFOR JANE".into(), feed: "agenda".into(), done: false }
}

#[test]
fn the_register_becomes_courses_tasks_grades_and_notices() {
    assert_eq!(tidy_subject("  MATEMATICA  E FISICA"), "Matematica e fisica");
    assert_eq!(tidy_person("DE ROSSI ANNA-MARIA"), "De Rossi Anna-Maria");
    let db = Rc::new(Database::in_memory().unwrap());
    let register = Arc::new(FakeRegister::default());
    register.agenda.lock().unwrap().extend([agenda("e1", "Esercizi pag. 12\nall of them", "2026-09-25"), agenda("e2", "Verifica", "2026-09-28")]);
    let school_repo = Rc::new(SqliteSchool(db.clone()));
    let service = SchoolSyncService {
        register: register.clone(), credentials: Rc::new(Keyring::default()), courses: Rc::new(SqliteCourses(db.clone())),
        lookup: Rc::new(SqliteCourses(db.clone())), tasks: Rc::new(SqliteTasks(db.clone())), records: school_repo.clone(),
        links: school_repo.clone(), settings: Rc::new(SqliteSettings(db.clone())),
        clock: Rc::new(Fixed(Cell::new(day("2026-09-23").and_hms_opt(12, 0, 0).unwrap()))), bus: Rc::new(ChangeBus::default()),
    };
    assert!(service.connect("S123", "bad").is_err());
    assert_eq!(service.connect("S123", "pw").unwrap().student_name, "ANNA ROSSI");
    let report = service.sync().unwrap();
    assert!(report.first_sync && report.has_news());
    assert_eq!((report.courses_created, report.new_tasks.len(), report.new_grades.len(), report.new_notices.len(), report.new_absences), (1, 2, 1, 1, 1));
    let courses = SqliteCourses(db.clone()).list().unwrap();
    assert_eq!((courses[0].name.as_str(), courses[0].teacher.as_str()), ("Matematica", "Okafor Jane"));
    let t = &report.new_tasks[0];
    assert_eq!((t.title.as_str(), t.course_id), ("Esercizi pag. 12", courses[0].id));
    assert!(t.details.ends_with("— Okafor Jane"));
    // read here: stays read after the next sync; the teacher deletes e2: it goes
    service.mark_notice_read("classeviva:notice:n1").unwrap();
    register.agenda.lock().unwrap().retain(|a| a.id != "e2");
    let again = service.sync().unwrap();
    assert!(!again.first_sync);
    assert_eq!((again.removed_tasks, again.new_tasks.len(), again.new_grades.len()), (1, 0, 0));
    assert!(school_repo.notices().unwrap()[0].read);
    assert_eq!(school_repo.school_days().unwrap(), vec![day("2026-09-23")]);
    assert_eq!(service.status().student_name, "ANNA ROSSI");
}
