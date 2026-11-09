//! Writing an agenda as an iCalendar file (RFC 5545; port of `quire/domain/ical.py`). Times are floating (local), so
//! they read the same wherever the file is opened, like the times in Quire.

use chrono::{Duration, NaiveDate, NaiveDateTime};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEntry {
    /// Stays the same for the same thing, so a calendar updates it instead of adding a copy.
    pub uid: String,
    pub title: String,
    pub day: NaiveDate,
    /// Minutes after midnight.
    pub start: i32,
    pub end: i32,
    pub all_day: bool,
    pub details: String,
    pub place: String,
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace(';', r"\;").replace(',', "\\,").replace("\r\n", "\n").replace('\n', "\\n")
}

/// Lines of at most 75 bytes, continued with a leading space, never cutting a character in half.
fn fold(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let (mut current, mut size) = (String::new(), 0);
    for ch in line.chars() {
        let width = ch.len_utf8();
        if size + width > if out.is_empty() { 75 } else { 74 } {
            out.push(std::mem::take(&mut current));
            size = 0;
        }
        current.push(ch);
        size += width;
    }
    out.push(current);
    out.iter().enumerate().map(|(i, piece)| if i == 0 { piece.clone() } else { format!(" {piece}") }).collect()
}

fn stamp(day: NaiveDate, minute: i32) -> String {
    let moment = day.and_hms_opt(0, 0, 0).expect("midnight") + Duration::minutes(i64::from(minute));
    moment.format("%Y%m%dT%H%M%S").to_string()
}

pub fn to_ics(entries: &[CalendarEntry], name: &str, now: NaiveDateTime) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Quezka//Quire//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        format!("X-WR-CALNAME:{}", escape(name)),
    ];
    // By the day: the same agenda gives the same file, so nothing reloads.
    let created = now.format("%Y%m%dT000000").to_string();
    for e in entries {
        lines.extend(["BEGIN:VEVENT".into(), format!("UID:{}", e.uid), format!("DTSTAMP:{created}"), format!("SUMMARY:{}", escape(&e.title))]);
        if e.all_day {
            lines.push(format!("DTSTART;VALUE=DATE:{}", e.day.format("%Y%m%d")));
            lines.push(format!("DTEND;VALUE=DATE:{}", (e.day + Duration::days(1)).format("%Y%m%d")));
        } else {
            lines.push(format!("DTSTART:{}", stamp(e.day, e.start)));
            lines.push(format!("DTEND:{}", stamp(e.day, e.end)));
        }
        if !e.details.is_empty() {
            lines.push(format!("DESCRIPTION:{}", escape(&e.details)));
        }
        if !e.place.is_empty() {
            lines.push(format!("LOCATION:{}", escape(&e.place)));
        }
        lines.push("END:VEVENT".into());
    }
    lines.push("END:VCALENDAR".into());
    let mut out = lines.iter().flat_map(|l| fold(l)).collect::<Vec<_>>().join("\r\n");
    out.push_str("\r\n");
    out
}
