//! The Rust rules give the Python app's answers (fixtures from `tools/quire_fixtures.py`).

use chrono::NaiveDate;
use quire::domain::{finance, ical, model, register, sleep, water};
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/rules.json")).unwrap()
}

fn day(v: &Value) -> NaiveDate {
    NaiveDate::parse_from_str(v.as_str().unwrap(), "%Y-%m-%d").unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

#[test]
fn water() {
    let all = fixtures();
    for c in all["water_goal"].as_array().unwrap() {
        assert_eq!(water::water_goal_ml(f(&c[0]), f(&c[1])).unwrap(), i(&c[2]), "{c}");
    }
    for c in all["expected"].as_array().unwrap() {
        assert_eq!(water::expected_ml(i(&c[0]), i(&c[1]) as i32, water::DEFAULT_START, water::DEFAULT_END), i(&c[2]), "{c}");
    }
    for c in all["reminder"].as_array().unwrap() {
        let last = c[3].as_i64().map(|v| v as i32);
        let due = water::reminder_due(i(&c[0]), i(&c[1]), i(&c[2]) as i32, last, water::DEFAULT_START, water::DEFAULT_END);
        assert_eq!(due, c[4].as_bool().unwrap(), "{c}");
    }
}

#[test]
fn sleep() {
    let all = fixtures();
    for c in all["sleep"].as_array().unwrap() {
        let logs: Vec<sleep::SleepLog> = c["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| sleep::SleepLog { day: day(&l[0]), asleep: i(&l[1]) as i32, woke: i(&l[2]) as i32, quality: l[3].as_u64().map(|q| q as u8), note: String::new() })
            .collect();
        let s = sleep::sleep_stats(&logs, f(&c["goal"]));
        let want = &c["stats"];
        assert_eq!(s.nights as i64, i(&want["nights"]), "{c}");
        assert_eq!(i64::from(s.average_minutes), i(&want["average_minutes"]), "{c}");
        assert_eq!(s.average_bedtime.map(i64::from), want["average_bedtime"].as_i64(), "{c}");
        assert_eq!(s.average_wake.map(i64::from), want["average_wake"].as_i64(), "{c}");
        match (s.average_quality, want["average_quality"].as_f64()) {
            (Some(a), Some(b)) => assert!((a - b).abs() < 1e-12),
            (a, b) => assert_eq!(a, b),
        }
        assert_eq!(s.enough as i64, i(&want["enough"]));
        assert_eq!(i64::from(s.shortfall_minutes), i(&want["shortfall_minutes"]));
        assert_eq!(i64::from(s.bedtime_spread), i(&want["bedtime_spread"]), "{c}");
        assert_eq!(s.streak as i64, i(&want["streak"]), "{c}");
    }
    for c in all["bedtime"].as_array().unwrap() {
        assert_eq!(i64::from(sleep::bedtime_for(i(&c[0]) as i32, f(&c[1]))), i(&c[2]), "{c}");
    }
}

#[test]
fn register() {
    let all = fixtures();
    for c in all["needed"].as_array().unwrap() {
        let marks: Vec<f64> = c[0].as_array().unwrap().iter().map(f).collect();
        assert_eq!(register::needed_grade(&marks, f(&c[1]), i(&c[2]) as u32), f(&c[3]), "{c}");
    }
    let running = &all["running"];
    let points: Vec<(NaiveDate, f64)> = running[0].as_array().unwrap().iter().map(|p| (day(&p[0]), f(&p[1]))).collect();
    let want: Vec<(NaiveDate, f64)> = running[1].as_array().unwrap().iter().map(|p| (day(&p[0]), f(&p[1]))).collect();
    assert_eq!(register::running_average(&points), want);
    for c in all["lesson_hours"].as_array().unwrap() {
        assert_eq!(i64::from(register::lesson_hours(i(&c[0]) as i32)), i(&c[1]), "{c}");
    }
}

#[test]
fn money() {
    for c in fixtures()["cents"].as_array().unwrap() {
        assert_eq!(finance::parse_cents(c[0].as_str().unwrap()).unwrap(), i(&c[1]), "{c}");
    }
}

#[test]
fn calendar_file() {
    let d = |m, dd| NaiveDate::from_ymd_opt(2026, m, dd).unwrap();
    let entries = [
        ical::CalendarEntry {
            uid: "e1@quire".into(),
            title: "Maths; test, chapter 4".into(),
            day: d(3, 10),
            start: 510,
            end: 600,
            all_day: false,
            details: "Bring the calculator\nand a pencil".into(),
            place: "Room B1".into(),
        },
        ical::CalendarEntry {
            uid: "e2@quire".into(),
            title: format!("Gita a Firenze — {}", "очень длинное название ".repeat(4)),
            day: d(3, 11),
            start: 0,
            end: 24 * 60,
            all_day: true,
            details: String::new(),
            place: String::new(),
        },
    ];
    let now = d(3, 9).and_hms_opt(15, 30, 0).unwrap();
    assert_eq!(ical::to_ics(&entries, "Quire, agenda", now), fixtures()["ics"].as_str().unwrap());
}

#[test]
fn note_titles_and_snippets() {
    for c in fixtures()["notes"].as_array().unwrap() {
        let body = c[0].as_str().unwrap();
        assert_eq!(model::derive_note_title(body), c[1].as_str().unwrap(), "{body:?}");
        assert_eq!(model::note_snippet(body, 90), c[2].as_str().unwrap(), "{body:?}");
        assert_eq!(model::note_snippet(body, 20), c[3].as_str().unwrap(), "{body:?}");
    }
}
