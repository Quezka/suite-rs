//! dates + times as text (= python `presentation/formatting.py`)

use chrono::{Datelike, NaiveDate};

use super::i18n::{month_of, month_short, t, tf, weekday_name, weekday_short};
use crate::domain::model::TimeRange;

pub fn fmt_min(minutes: i32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

pub fn fmt_range(r: &TimeRange) -> String {
    format!("{}–{}", fmt_min(r.start), fmt_min(r.end))
}

/// russian day names lowercase mid-sentence; heading -> capital
pub fn capitalised(text: &str) -> String {
    let mut c = text.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// "Monday, 16 November"
pub fn long_date(d: NaiveDate) -> String {
    capitalised(&format!("{}, {} {}", weekday_name(d), d.day(), month_of(d)))
}

pub fn relative_date(d: NaiveDate, today: NaiveDate) -> String {
    let delta = (d - today).num_days();
    match delta {
        0 => t("Today"),
        1 => t("Tomorrow"),
        -1 => t("Yesterday"),
        2..=6 => capitalised(weekday_name(d)),
        _ => {
            let text = format!("{} {} {}", weekday_short(d), d.day(), month_short(d));
            if d.year() == today.year() { text } else { format!("{text} {}", d.year()) }
        }
    }
}

/// "was due yesterday" | "was due Mon 21 Sep"
pub fn was_due(d: NaiveDate, today: NaiveDate) -> String {
    let mut when = relative_date(d, today);
    if when == t("Yesterday") {
        when = when.to_lowercase();
    }
    tf("was due {when}", &[("when", &when)])
}

pub fn fmt_duration(minutes: i32) -> String {
    let (hours, rest) = (minutes / 60, minutes % 60);
    if hours == 0 {
        tf("{minutes} min", &[("minutes", &rest.to_string())])
    } else if rest != 0 {
        tf("{hours} h {minutes:02d}", &[("hours", &hours.to_string()), ("minutes:02d", &format!("{rest:02}"))])
    } else {
        tf("{hours} h", &[("hours", &hours.to_string())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_dates_and_times() {
        quezka_kit::i18n::set_language(quezka_kit::i18n::Lang::En);
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        assert_eq!(fmt_min(9 * 60 + 5), "09:05");
        assert_eq!(long_date(today), "Monday, 16 November");
        assert_eq!(relative_date(today + chrono::Duration::days(3), today), "Thursday");
        assert_eq!(relative_date(today + chrono::Duration::days(30), today), "Wed 16 Dec");
        assert_eq!(was_due(today - chrono::Duration::days(1), today), "was due yesterday");
        assert_eq!(fmt_duration(95), "1 h 35");
        assert_eq!(fmt_duration(120), "2 h");
    }
}
