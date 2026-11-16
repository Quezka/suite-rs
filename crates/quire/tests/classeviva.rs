//! Classeviva against canned JSON (the same cases and data as the Python app's tests/test_classeviva.py).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use quire::application::ports::{Credentials, RegisterError, SchoolRegister};
use quire::domain::model::TaskKind;
use quire::infrastructure::classeviva::{plain_text, school_year, ClassevivaRegister, BASE_URL};
use quire::infrastructure::http::Http;
use serde_json::{json, Value};

struct FakeServer {
    routes: HashMap<String, (u16, Value)>,
    requests: Mutex<Vec<(String, String, Vec<(String, String)>, Option<String>)>>,
}

impl Http for FakeServer {
    fn send(&self, method: &str, url: &str, headers: &[(&str, String)], body: Option<String>) -> Result<(u16, String), String> {
        let path = url.trim_start_matches(BASE_URL).to_string();
        self.requests.lock().unwrap().push((method.into(), path.clone(), headers.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(), body));
        match self.routes.get(&path) {
            Some((status, v)) => Ok((*status, v.to_string())),
            None => Err("no route to host".into()),
        }
    }
}

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn login() -> Value {
    json!({"ident": "S1234567X", "firstName": "MARIO", "lastName": "ROSSI", "token": "tok"})
}

fn signed_in(routes: Vec<(&str, u16, Value)>, today: fn() -> NaiveDate) -> (ClassevivaRegister, Arc<FakeServer>, String) {
    let mut map: HashMap<String, (u16, Value)> = routes.into_iter().map(|(p, s, v)| (p.to_string(), (s, v))).collect();
    map.insert("/auth/login".into(), (200, login()));
    let server = Arc::new(FakeServer { routes: map, requests: Mutex::new(Vec::new()) });
    let mut register = ClassevivaRegister::new(server.clone());
    register.today = today;
    let account = register.login(&Credentials { username: "S1234567X".into(), password: "pw".into() }).unwrap();
    (register, server, account.student_name)
}

fn sept23() -> NaiveDate {
    d("2026-09-23")
}

#[test]
fn login_names_the_student_and_sends_the_key() {
    let (_, server, name) = signed_in(vec![], sept23);
    assert_eq!(name, "Mario Rossi");
    let req = &server.requests.lock().unwrap()[0];
    assert_eq!(req.0, "POST");
    assert!(req.2.iter().any(|(k, v)| k == "Z-Dev-Apikey" && v == "Tg1NWEwNGIgIC0K"));
    assert_eq!(serde_json::from_str::<Value>(req.3.as_ref().unwrap()).unwrap(), json!({"ident": null, "pass": "pw", "uid": "S1234567X"}));
}

#[test]
fn wrong_password_and_no_network_read_well() {
    let server = Arc::new(FakeServer { routes: [("/auth/login".to_string(), (422, json!({"info": "WrongCredentials"})))].into(), requests: Mutex::new(vec![]) });
    let creds = Credentials { username: "x".into(), password: "y".into() };
    assert!(matches!(ClassevivaRegister::new(server).login(&creds), Err(RegisterError::Authentication(_))));
    let offline = Arc::new(FakeServer { routes: HashMap::new(), requests: Mutex::new(vec![]) });
    match ClassevivaRegister::new(offline).login(&creds) {
        Err(RegisterError::Failed(m)) => assert!(m.contains("internet")),
        other => panic!("{other:?}"),
    }
    let nobody = Arc::new(FakeServer { routes: HashMap::new(), requests: Mutex::new(vec![]) });
    match ClassevivaRegister::new(nobody).grades() {
        Err(RegisterError::Failed(m)) => assert!(m.contains("Not signed in")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn agenda_is_parsed_and_classified() {
    let agenda = json!({"agenda": [
        {"evtId": 1, "evtCode": "AGHW", "evtDatetimeBegin": "2026-09-25T11:00:00+02:00", "notes": "P. 467 n. 210",
         "authorName": "BIANCHI ANNA", "subjectId": 206168, "subjectDesc": "MATEMATICA"},
        {"evtId": 2, "evtCode": "AGNT", "evtDatetimeBegin": "2026-09-28T00:00:00+02:00", "notes": "Verifica di storia sul capitolo 2",
         "authorName": "VERDI", "subjectId": null, "subjectDesc": null},
        {"evtId": 3, "evtCode": "AGNT", "evtDatetimeBegin": "2026-09-29T00:00:00+02:00", "notes": "Portare il libro", "authorName": "VERDI"},
    ]});
    let (register, server, _) = signed_in(vec![("/students/1234567/agenda/all/20260901/20261231", 200, agenda)], sept23);
    // asking from late August must not reach before 1 September
    let items = register.assignments(d("2026-08-24"), d("2026-12-31")).unwrap();
    assert!(server.requests.lock().unwrap().last().unwrap().2.iter().any(|(k, v)| k == "Z-Auth-Token" && v == "tok"));
    let got: Vec<(&str, TaskKind, NaiveDate)> = items.iter().map(|i| (i.id.as_str(), i.kind, i.day)).collect();
    assert_eq!(got, vec![("1", TaskKind::Homework, d("2026-09-25")), ("2", TaskKind::Exam, d("2026-09-28")), ("3", TaskKind::Task, d("2026-09-29"))]);
    assert_eq!((items[0].subject_id.as_deref(), items[1].subject_id.as_deref()), (Some("206168"), None));
}

#[test]
fn grades_lessons_subjects_and_summer() {
    let grades = json!({"grades": [
        {"subjectId": 206166, "subjectDesc": "ARTE", "evtId": 223044, "evtDate": "2026-09-20", "decimalValue": 8, "displayValue": "8",
         "notesForFamily": "orale", "canceled": false, "periodDesc": "Trimestre", "componentDesc": "Orale"},
        {"subjectId": 206166, "subjectDesc": "ARTE", "evtId": 223045, "evtDate": "2026-09-21", "decimalValue": null, "displayValue": "+", "canceled": true},
    ]});
    let lessons = json!({"lessons": [
        {"evtId": 5, "evtDate": "2026-09-22", "evtHPos": 2, "authorName": "NERI", "subjectId": 206167, "subjectDesc": "FISICA", "lessonArg": "Moto rettilineo"},
        {"evtId": 5, "evtDate": "2026-09-22", "evtHPos": 3, "authorName": "NERI", "subjectId": 206167, "subjectDesc": "FISICA", "lessonArg": "Moto rettilineo"},
    ]});
    let subjects = json!({"subjects": [{"id": 217039, "description": "STORIA", "teachers": [{"teacherId": "A1", "teacherName": "COGNOME NOME"}]}]});
    let (register, _, _) = signed_in(vec![("/students/1234567/grades", 200, grades), ("/students/1234567/lessons/20260915/20260923", 200, lessons),
                                         ("/students/1234567/subjects", 200, subjects)], sept23);
    let g = register.grades().unwrap();
    assert_eq!((g[0].value, g[0].display.as_str(), g[0].component.as_str(), g[0].notes.as_str()), (Some(8.0), "8", "Orale", "orale"));
    assert!(g[1].value.is_none() && g[1].cancelled);
    let l = register.lessons(d("2026-09-15"), d("2026-09-23")).unwrap();
    assert_eq!((l.len(), l[0].topic.as_str(), l[0].hour, l[0].teacher.as_str()), (1, "Moto rettilineo", 2, "NERI"));
    let s = register.subjects().unwrap();
    assert_eq!((s[0].name.as_str(), s[0].teachers.clone()), ("STORIA", vec!["COGNOME NOME".to_string()]));
    // summer holidays: nothing is asked for
    let (summer, server, _) = signed_in(vec![], || "2026-07-15".parse().unwrap());
    assert!(summer.assignments(d("2026-07-01"), d("2026-08-31")).unwrap().is_empty());
    assert_eq!(server.requests.lock().unwrap().len(), 1); // only the login
    assert_eq!(school_year(d("2026-09-01")), (d("2026-09-01"), d("2027-06-30")));
    assert_eq!(school_year(d("2026-08-31")), (d("2025-09-01"), d("2026-06-30")));
}

#[test]
fn grades_fall_back_to_the_overview_and_out_of_year_is_empty() {
    let moved = json!({"error": "102:CvvRestApi/wrong uri"});
    let overview = json!({"grades": [{"subjectId": 1, "subjectDesc": "ARTE", "evtId": 9, "evtDate": "2026-09-20", "decimalValue": 7, "displayValue": "7"}]});
    let (register, _, _) = signed_in(vec![("/students/1234567/grades", 404, moved), ("/students/1234567/overview/all/20260901/20260923", 200, overview)], sept23);
    assert_eq!(register.grades().unwrap()[0].value, Some(7.0));
    let outside = json!({"error": "122:CvvRestApi/invalid date range"});
    let (register, _, _) = signed_in(vec![("/students/1234567/lessons/20260915/20260923", 404, outside)], sept23);
    assert!(register.lessons(d("2026-09-15"), d("2026-09-23")).unwrap().is_empty());
}

#[test]
fn notices_open_as_plain_text() {
    assert_eq!(plain_text("<p>Gita a <b>Roma</b></p><ul><li>pranzo&nbsp;al sacco</li></ul>"), "Gita a Roma\n• pranzo al sacco");
    let notice_item = json!({"item": {"text": "<p>Ciao &amp; grazie</p>"}});
    let board = json!({"items": [{"evtCode": "CF", "pubId": 77, "cntTitle": "  Gita   scolastica ", "pubDT": "2026-09-20T10:00:00+02:00",
                                  "readStatus": false, "attachments": [{"fileName": "a.pdf"}]},
                                 {"evtCode": "CF", "pubId": 78, "cntStatus": "deleted"}]});
    let (register, _, _) = signed_in(vec![("/students/1234567/noticeboard", 200, board), ("/students/1234567/noticeboard/read/CF/77/101", 200, notice_item)], sept23);
    let notices = register.notices().unwrap();
    assert_eq!((notices.len(), notices[0].title.as_str(), notices[0].external_id.as_str()), (1, "Gita scolastica", "CF:77"));
    assert_eq!(notices[0].attachments[0].number, 1);
    assert_eq!(register.open_notice(&notices[0]).unwrap(), "Ciao & grazie");
}
