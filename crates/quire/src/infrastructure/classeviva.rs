//! Classeviva (Spaggiari), the Italian school register, through the REST API behind its mobile app (port of
//! `infrastructure/classeviva.py`). Not officially documented: endpoints and headers follow the community reference
//! github.com/Lioydiano/Classeviva-Official-Endpoints. Tests use canned JSON, never the network.

use std::sync::{Arc, Mutex};

use chrono::{Datelike, NaiveDate};
use fancy_regex::Regex;
use serde_json::{json, Value};

use super::http::Http;
use crate::application::ports::{Credentials, RegisterAccount, RegisterError, RegisterResult, RemoteAssignment, RemoteGrade,
                                RemoteLesson, RemoteSubject, SchoolRegister};
use crate::domain::model::TaskKind;
use crate::domain::register::{Absence, AbsenceKind, Book, DocumentKind, Notice, NoticeAttachment, SchoolDocument};

pub const BASE_URL: &str = "https://web.spaggiari.eu/rest/v1";
const USER_AGENT: &str = "CVVS/std/4.2.3 Android/12";
const API_KEY: &str = "Tg1NWEwNGIgIC0K";
const HOMEWORK_CODE: &str = "AGHW";
const SCHOOL_DAY: &str = "SD";

fn re(cell: &'static std::sync::OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid"))
}

/// Agenda notes that are really homework.
fn homework_words(text: &str) -> bool {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    re(&R, r"(?i)\b(compit\w*|eserciz\w*|es\.|pag\.|pagg?\b|pagin\w*|studi\w*|legger\w*|ripass\w*|svolger\w*|homework|exercis\w*)")
        .is_match(text).unwrap_or(false)
}

/// Agenda notes announcing a test rather than homework.
fn exam_words(text: &str) -> bool {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    re(&R, r"(?i)\b(verific\w*|compit\w* in classe|interrogazion\w*|test|prova|prove|esame|esami|simulazione)\b")
        .is_match(text).unwrap_or(false)
}

/// The school year Classeviva serves data for: 1 September to 30 June.
pub fn school_year(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = if today.month() >= 9 { today.year() } else { today.year() - 1 };
    (NaiveDate::from_ymd_opt(start, 9, 1).unwrap(), NaiveDate::from_ymd_opt(start + 1, 6, 30).unwrap())
}

/// Trim a range to the current school year; None if nothing is left (summer).
pub fn clamp_to_school_year(first: NaiveDate, last: NaiveDate, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let (start, end) = school_year(today);
    let (f, l) = (first.max(start), last.min(end));
    (f <= l).then_some((f, l))
}

fn day(v: &Value) -> Option<NaiveDate> {
    let s = v.as_str()?;
    s.get(..10)?.parse().ok()
}

fn stamp(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn id(v: &Value, k: &str) -> Option<String> {
    match v.get(k) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

fn unescape(s: &str) -> String {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    re(&R, r"&(#\d+|#[xX][0-9a-fA-F]+|amp|lt|gt|quot|apos|nbsp|#39);").replace_all(s, |c: &fancy_regex::Captures| {
        let e = &c[1];
        match e {
            "amp" => "&".into(), "lt" => "<".into(), "gt" => ">".into(), "quot" => "\"".into(),
            "apos" | "#39" => "'".into(), "nbsp" => "\u{a0}".into(),
            _ => {
                let n = if e.starts_with("#x") || e.starts_with("#X") { u32::from_str_radix(&e[2..], 16).ok() } else { e[1..].parse().ok() };
                n.and_then(char::from_u32).map(String::from).unwrap_or_else(|| c[0].to_string())
            }
        }
    }).into_owned()
}

/// Notice text as plain text: some schools write it with HTML markup.
pub fn plain_text(text: &str) -> String {
    static TAG: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static BREAK: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static LI: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ANY: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static TRAIL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static GAPS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let mut t = text.to_string();
    if re(&TAG, r"<[a-zA-Z/][^>]*>").is_match(&t).unwrap_or(false) {
        t = re(&BREAK, r"(?i)<br\s*/?>|</(p|div|li|tr|h[1-6])>").replace_all(&t, "\n").into_owned();
        t = re(&LI, r"(?i)<li[^>]*>").replace_all(&t, "• ").into_owned();
        t = re(&ANY, r"<[^>]+>").replace_all(&t, "").into_owned();
    }
    t = unescape(&t).replace("\r\n", "\n").replace('\u{a0}', " ");
    t = re(&TRAIL, r"[ \t]+\n").replace_all(&t, "\n").into_owned();
    re(&GAPS, r"\n{3,}").replace_all(&t, "\n\n").trim().to_string()
}

enum Refusal {
    Register(RegisterError),
    /// Error 122: the date range falls outside the school year.
    OutsideSchoolYear,
    /// Error 102: the endpoint doesn't exist (it may have moved).
    WrongUri,
}

impl From<RegisterError> for Refusal {
    fn from(e: RegisterError) -> Self {
        Self::Register(e)
    }
}

fn fail(r: Refusal, what: &str) -> RegisterError {
    match r {
        Refusal::Register(e) => e,
        Refusal::OutsideSchoolYear => RegisterError::Failed("Outside the school year.".into()),
        Refusal::WrongUri => RegisterError::Failed(format!("Classeviva doesn't know {what} any more (the endpoint may have moved).")),
    }
}

pub struct ClassevivaRegister {
    pub http: Arc<dyn Http>,
    pub today: fn() -> NaiveDate,
    /// (token, student id) once signed in.
    session: Mutex<(Option<String>, Option<String>)>,
}

impl ClassevivaRegister {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self { http, today: || chrono::Local::now().date_naive(), session: Mutex::new((None, None)) }
    }

    fn send(&self, method: &str, path: &str, body: Option<Value>) -> Result<Vec<u8>, Refusal> {
        let mut headers = vec![("User-Agent", USER_AGENT.to_string()), ("Z-Dev-Apikey", API_KEY.to_string()),
                               ("Content-Type", "application/json".to_string())];
        if let Some(t) = &self.session.lock().unwrap().0 {
            headers.push(("Z-Auth-Token", t.clone()));
        }
        let (status, payload) = self.http.send_bytes(method, &format!("{BASE_URL}{path}"), &headers, body.map(|b| b.to_string()))
            .map_err(|e| RegisterError::Failed(format!("Couldn't reach Classeviva ({e}). Check your internet connection.")))?;
        if (200..300).contains(&status) {
            return Ok(payload);
        }
        let detail: Value = serde_json::from_slice(&payload).unwrap_or(json!({}));
        let code = detail.get("error").map(|e| e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())).unwrap_or_default();
        let info = detail.get("info").and_then(Value::as_str).map(str::to_string).filter(|i| !i.is_empty()).unwrap_or_else(|| code.clone());
        if path == "/auth/login" && matches!(status, 401 | 403 | 422) {
            return Err(RegisterError::Authentication("Classeviva didn't accept that username and password.".into()).into());
        }
        if matches!(status, 401 | 403) {
            return Err(RegisterError::Authentication("Your Classeviva session was refused. Try connecting your account again.".into()).into());
        }
        if status == 404 && code.starts_with("122") {
            return Err(Refusal::OutsideSchoolYear);
        }
        if status == 404 && code.starts_with("102") {
            return Err(Refusal::WrongUri);
        }
        let suffix = if info.is_empty() { String::new() } else { format!(": {info}") };
        Err(RegisterError::Failed(format!("Classeviva answered with an error ({status}{suffix}).")).into())
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, Refusal> {
        let payload = self.send(method, path, body)?;
        if payload.is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_slice(&payload).map_err(|_| RegisterError::Failed("Classeviva sent a response Quire couldn't read.".into()).into())
    }

    fn student_path(&self, path: &str) -> RegisterResult<String> {
        match &self.session.lock().unwrap().1 {
            Some(s) => Ok(format!("/students/{s}{path}")),
            None => Err(RegisterError::Failed("Not signed in to Classeviva.".into())),
        }
    }

    fn get(&self, path: &str) -> RegisterResult<Value> {
        let full = self.student_path(path)?;
        self.request("GET", &full, None).map_err(|r| fail(r, path.split('/').nth(1).unwrap_or(path)))
    }

    /// GET a dated endpoint, keeping the range inside the school year.
    fn ranged(&self, path: &str, first: NaiveDate, last: NaiveDate) -> RegisterResult<Value> {
        let Some((f, l)) = clamp_to_school_year(first, last, (self.today)()) else { return Ok(json!({})) };
        let full = self.student_path(&format!("{path}/{}/{}", stamp(f), stamp(l)))?;
        match self.request("GET", &full, None) {
            Ok(v) => Ok(v),
            Err(Refusal::OutsideSchoolYear) => Ok(json!({})),
            Err(r) => Err(fail(r, path.split('/').nth(1).unwrap_or(path))),
        }
    }

    fn list<'a>(v: &'a Value, k: &str) -> &'a [Value] {
        v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }
}

impl SchoolRegister for ClassevivaRegister {
    fn name(&self) -> String {
        "Classeviva".into()
    }

    fn login(&self, c: &Credentials) -> RegisterResult<RegisterAccount> {
        *self.session.lock().unwrap() = (None, None);
        let mut body = json!({"ident": null, "pass": c.password, "uid": c.username});
        let mut data = self.request("POST", "/auth/login", Some(body.clone())).map_err(|r| fail(r, "auth"))?;
        if data.get("token").is_none() {
            if let Some(first) = data.get("choices").and_then(Value::as_array).and_then(|a| a.first()) {
                // accounts linked to several profiles (a parent with two children) pick one: the first
                body["ident"] = first.get("ident").cloned().unwrap_or(Value::Null);
                data = self.request("POST", "/auth/login", Some(body)).map_err(|r| fail(r, "auth"))?;
            }
        }
        let token = data.get("token").and_then(Value::as_str).filter(|t| !t.is_empty())
            .ok_or_else(|| RegisterError::Failed("Classeviva didn't open a session for this account.".into()))?;
        let student: String = text(&data, "ident").chars().filter(char::is_ascii_digit).collect();
        if student.is_empty() {
            return Err(RegisterError::Failed("Classeviva didn't say which student this account belongs to.".into()));
        }
        *self.session.lock().unwrap() = (Some(token.into()), Some(student));
        let name = [text(&data, "firstName"), text(&data, "lastName")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        let upper = name.chars().any(char::is_alphabetic) && !name.chars().any(char::is_lowercase);
        Ok(RegisterAccount { student_name: if upper { crate::application::school::tidy_person(&name) } else { name } })
    }

    fn subjects(&self) -> RegisterResult<Vec<RemoteSubject>> {
        let data = self.get("/subjects")?;
        Ok(Self::list(&data, "subjects").iter().map(|x| RemoteSubject {
            id: id(x, "id").unwrap_or_default(), name: text(x, "description"),
            teachers: Self::list(x, "teachers").iter().map(|t| text(t, "teacherName")).collect(),
        }).collect())
    }

    fn assignments(&self, first: NaiveDate, last: NaiveDate) -> RegisterResult<Vec<RemoteAssignment>> {
        let data = self.ranged("/agenda/all", first, last)?;
        Ok(Self::list(&data, "agenda").iter().filter_map(|e| {
            let t = text(e, "notes").trim().to_string();
            let kind = if text(e, "evtCode") == HOMEWORK_CODE { TaskKind::Homework } else if exam_words(&t) { TaskKind::Exam }
                       else if homework_words(&t) { TaskKind::Homework } else { TaskKind::Task };
            Some(RemoteAssignment { id: id(e, "evtId")?, day: day(e.get("evtDatetimeBegin")?)?, kind, text: t,
                subject_id: id(e, "subjectId"), subject_name: text(e, "subjectDesc"), author: text(e, "authorName"),
                feed: "agenda".into(), done: false })
        }).collect())
    }

    fn homework(&self) -> RegisterResult<Vec<RemoteAssignment>> {
        let data = self.get("/homeworks")?;
        Ok(Self::list(&data, "items").iter().filter_map(|h| {
            let due = h.get("expiryDate").filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).or_else(|| h.get("assignmentDate"))?;
            Some(RemoteAssignment { id: id(h, "evtId")?, day: day(due)?, kind: TaskKind::Homework, text: text(h, "homeworkDesc").trim().into(),
                subject_id: id(h, "subjectId"), subject_name: text(h, "subjectDesc"), author: text(h, "teacherName"),
                feed: "homework".into(), done: h.get("homeworkDone").and_then(Value::as_bool).unwrap_or(false) })
        }).collect())
    }

    fn grades(&self) -> RegisterResult<Vec<RemoteGrade>> {
        let path = self.student_path("/grades")?;
        let data = match self.request("GET", &path, None) {
            Ok(v) => v,
            Err(Refusal::WrongUri) => {
                // some accounts see /grades answer "wrong uri"; the overview carries grades too
                let today = (self.today)();
                let overview = self.student_path(&format!("/overview/all/{}/{}", stamp(school_year(today).0), stamp(today)))?;
                match self.request("GET", &overview, None) {
                    Ok(v) => v,
                    Err(Refusal::WrongUri) => return Err(RegisterError::Failed("Classeviva's grades endpoint has moved; Quire needs an update to read grades.".into())),
                    Err(r) => return Err(fail(r, "grades")),
                }
            }
            Err(r) => return Err(fail(r, "grades")),
        };
        Ok(Self::list(&data, "grades").iter().filter_map(|g| Some(RemoteGrade {
            id: id(g, "evtId")?, day: day(g.get("evtDate")?)?, subject_id: id(g, "subjectId"), subject_name: text(g, "subjectDesc"),
            display: text(g, "displayValue").trim().into(), value: g.get("decimalValue").and_then(Value::as_f64),
            component: text(g, "componentDesc"), period: text(g, "periodDesc"), notes: text(g, "notesForFamily").trim().into(),
            cancelled: g.get("canceled").and_then(Value::as_bool).unwrap_or(false),
        })).collect())
    }

    fn lessons(&self, first: NaiveDate, last: NaiveDate) -> RegisterResult<Vec<RemoteLesson>> {
        let data = self.ranged("/lessons", first, last)?;
        let mut seen = std::collections::HashSet::new();
        Ok(Self::list(&data, "lessons").iter().filter_map(|l| {
            let lid = id(l, "evtId")?;
            if !seen.insert(lid.clone()) {
                return None;
            }
            Some(RemoteLesson { id: lid, day: day(l.get("evtDate")?)?, subject_id: id(l, "subjectId"), subject_name: text(l, "subjectDesc"),
                topic: text(l, "lessonArg").trim().into(), teacher: text(l, "authorName"),
                hour: l.get("evtHPos").and_then(Value::as_i64).unwrap_or(0) as i32 })
        }).collect())
    }

    fn absences(&self) -> RegisterResult<Vec<Absence>> {
        let data = self.get("/absences/details")?;
        Ok(Self::list(&data, "events").iter().filter_map(|e| {
            let kind = match text(e, "evtCode").as_str() {
                "ABA0" => AbsenceKind::Absent, "ABR0" => AbsenceKind::Late, "ABR1" => AbsenceKind::ShortLate, "ABU0" => AbsenceKind::EarlyExit,
                _ => return None,
            };
            Some(Absence { external_id: id(e, "evtId")?, day: day(e.get("evtDate")?)?, kind,
                hour: e.get("evtHPos").and_then(Value::as_i64).map(|h| h as i32),
                justified: e.get("isJustified").and_then(Value::as_bool).unwrap_or(false), reason: text(e, "justifReasonDesc").trim().into(),
                hours: Self::list(e, "hoursAbsence").len() as i32, own_hour: None })
        }).collect())
    }

    fn notices(&self) -> RegisterResult<Vec<Notice>> {
        let data = self.get("/noticeboard")?;
        let today = (self.today)();
        Ok(Self::list(&data, "items").iter().filter(|n| text(n, "cntStatus") != "deleted").map(|n| {
            let (code, pubid) = (id(n, "evtCode").unwrap_or_default(), id(n, "pubId").unwrap_or_default());
            let published = n.get("pubDT").and_then(day).or_else(|| n.get("cntValidFrom").and_then(day)).unwrap_or(today);
            Notice {
                external_id: format!("{code}:{pubid}"), code, pub_id: pubid,
                title: text(n, "cntTitle").split_whitespace().collect::<Vec<_>>().join(" "), category: text(n, "cntCategory"),
                published, valid_until: n.get("cntValidTo").and_then(day), read: n.get("readStatus").and_then(Value::as_bool).unwrap_or(false),
                attachments: Self::list(n, "attachments").iter().enumerate().map(|(i, a)| NoticeAttachment {
                    number: a.get("attachNum").and_then(Value::as_i64).map_or(i as i32 + 1, |x| x as i32), file_name: text(a, "fileName"),
                }).collect(),
            }
        }).collect())
    }

    fn open_notice(&self, n: &Notice) -> RegisterResult<String> {
        // 101 is the "read" action the official app sends; the reply carries the text
        let path = self.student_path(&format!("/noticeboard/read/{}/{}/101", n.code, n.pub_id))?;
        let data = self.request("POST", &path, None).map_err(|r| fail(r, "noticeboard"))?;
        Ok(plain_text(&data.get("item").map(|i| text(i, "text")).unwrap_or_default()))
    }

    fn notice_attachment(&self, n: &Notice, number: i32) -> RegisterResult<Vec<u8>> {
        let path = self.student_path(&format!("/noticeboard/attach/{}/{}/{number}", n.code, n.pub_id))?;
        self.send("GET", &path, None).map_err(|r| fail(r, "noticeboard"))
    }

    fn books(&self) -> RegisterResult<Vec<Book>> {
        let data = self.get("/schoolbooks")?;
        Ok(Self::list(&data, "schoolbooks").iter().flat_map(|c| Self::list(c, "books").iter().map(|b| Book {
            isbn: id(b, "isbnCode").or_else(|| id(b, "bookId")).unwrap_or_default(),
            title: text(b, "title").split_whitespace().collect::<Vec<_>>().join(" "), subject: text(b, "subjectDesc"),
            author: text(b, "author"), publisher: text(b, "publisher"),
            volume: Some(text(b, "volume")).filter(|v| !v.is_empty()).unwrap_or_else(|| text(b, "subheading")),
            price: b.get("price").and_then(Value::as_f64), to_buy: b.get("toBuy").and_then(Value::as_bool).unwrap_or(false),
            owned: b.get("alreadyOwned").and_then(Value::as_bool).unwrap_or(false), new_adoption: b.get("newAdoption").and_then(Value::as_bool).unwrap_or(false),
        }).collect::<Vec<_>>()).collect())
    }

    fn documents(&self) -> RegisterResult<Vec<SchoolDocument>> {
        let path = self.student_path("/documents")?;
        let data = self.request("POST", &path, None).map_err(|r| fail(r, "documents"))?;
        let mut out: Vec<SchoolDocument> = Self::list(&data, "documents").iter().filter(|d| !text(d, "hash").is_empty()).map(|d| SchoolDocument {
            external_id: text(d, "hash"), title: text(d, "desc").trim().into(), kind: DocumentKind::Document, link: String::new(),
        }).collect();
        out.extend(Self::list(&data, "schoolReports").iter().filter_map(|r| {
            let link = Some(text(r, "viewLink")).filter(|l| !l.is_empty()).or_else(|| Some(text(r, "confirmLink")).filter(|l| !l.is_empty()))?;
            Some(SchoolDocument { external_id: link.clone(), title: text(r, "desc").trim().into(), kind: DocumentKind::Report, link })
        }));
        Ok(out)
    }

    fn document_file(&self, d: &SchoolDocument) -> RegisterResult<Vec<u8>> {
        let path = self.student_path(&format!("/documents/read/{}", d.external_id))?;
        self.send("POST", &path, None).map_err(|r| fail(r, "documents"))
    }

    fn school_days(&self) -> RegisterResult<Vec<NaiveDate>> {
        let data = self.get("/calendar/all")?;
        Ok(Self::list(&data, "calendar").iter().filter(|x| text(x, "dayStatus") == SCHOOL_DAY).filter_map(|x| x.get("dayDate").and_then(day)).collect())
    }
}
