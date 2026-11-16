//! quire words: table @ `messages.rs` (generated from python catalogues), rules @ `quezka_kit::i18n`

use chrono::{Datelike, NaiveDate};
use quezka_kit::i18n::{language, Lang, Names, EN_NAMES};

use super::messages::{IT_NAMES, MESSAGES, PLURALS, RU_NAMES};

quezka_kit::translations!(MESSAGES);

/// = python `C_`: word whose translation depends on use (`c("button", "Start")`)
pub fn c(context: &str, english: &str) -> String {
    let keyed = format!("{context}|{english}");
    let got = t(&keyed);
    if got == keyed { t(english) } else { got }
}

/// `3 lessons` | `3 урока`
pub fn plural(n: i64, word: &str, many: Option<&str>) -> String {
    quezka_kit::i18n::plural(PLURALS, language(), n, word, many)
}

pub fn names() -> &'static Names {
    match language() {
        Lang::It => &IT_NAMES,
        Lang::Ru => &RU_NAMES,
        Lang::En => &EN_NAMES,
    }
}

pub fn weekday_name(d: NaiveDate) -> &'static str {
    names().weekdays[d.weekday().num_days_from_monday() as usize]
}

pub fn weekday_short(d: NaiveDate) -> &'static str {
    names().weekdays_short[d.weekday().num_days_from_monday() as usize]
}

/// standalone: "September"
pub fn month_name(month: u32) -> &'static str {
    names().months[(month as usize).clamp(1, 12) - 1]
}

/// after day number: "23 September" | "23 сентября"
pub fn month_of(d: NaiveDate) -> &'static str {
    names().months_genitive[d.month0() as usize]
}

pub fn month_short(d: NaiveDate) -> &'static str {
    names().months_short[d.month0() as usize]
}

#[cfg(test)]
mod tests {
    #[test]
    fn catalogue_rows_are_complete() {
        quezka_kit::i18n::check_messages(super::MESSAGES).unwrap();
    }
}
