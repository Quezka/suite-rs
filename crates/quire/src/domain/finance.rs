//! Personal finance rules (port of `quire/domain/finance.py`). Amounts are whole cents, so they add up exactly.

use chrono::{Duration, NaiveDate};

use super::errors::{Checked, ValidationError};

/// One entry; more is a typo.
pub const MAX_CENTS: i64 = 100_000_000_00;
pub const UNCATEGORISED: &str = "";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub day: NaiveDate,
    /// Always positive; `income` says which way it went.
    pub cents: i64,
    pub income: bool,
    pub category: String,
    pub note: String,
    pub id: Option<i64>,
}

impl Transaction {
    pub fn validate(&self) -> Checked {
        if !(0 < self.cents && self.cents <= MAX_CENTS) {
            return Err(ValidationError("Enter an amount greater than zero."));
        }
        Ok(())
    }

    pub fn signed(&self) -> i64 {
        if self.income { self.cents } else { -self.cents }
    }
}

/// "12,50", "12.5", "1.234,56", "1,234.56" → cents. The last separator followed by one or two digits is the decimal
/// one; any other is only there to group thousands.
pub fn parse_cents(text: &str) -> Checked<i64> {
    let bad = || ValidationError("Enter an amount like 12,50.");
    let text: String = text.trim().chars().filter(|c| !matches!(c, ' ' | '€' | '$' | '£')).collect();
    let last = text.rfind([',', '.']);
    let (whole, cents) = match last {
        Some(at) if matches!(text.len() - at - 1, 1 | 2) => (text[..at].to_string(), format!("{:0<2}", &text[at + 1..])),
        _ => (text.clone(), "00".to_string()),
    };
    let whole: String = whole.chars().filter(|c| !matches!(c, ',' | '.')).collect();
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let whole_or_zero = if whole.is_empty() { "0" } else { whole.as_str() };
    if (whole.is_empty() && cents.trim_start_matches('0').is_empty()) || !digits(whole_or_zero) || !digits(&cents) {
        return Err(bad());
    }
    let w: i64 = whole_or_zero.parse().map_err(|_| bad())?;
    let c: i64 = cents.parse().map_err(|_| bad())?;
    Ok(w * 100 + c)
}

pub fn month_bounds(year: i32, month: u32) -> (NaiveDate, NaiveDate) {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("a month");
    let (ny, nm) = next_month(year, month);
    (first, NaiveDate::from_ymd_opt(ny, nm, 1).expect("a month") - Duration::days(1))
}

pub fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 { (year - 1, 12) } else { (year, month - 1) }
}

pub fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 { (year + 1, 1) } else { (year, month + 1) }
}

/// What went out, per category, biggest first.
pub fn by_category(transactions: &[Transaction]) -> Vec<(String, i64)> {
    let mut totals: Vec<(String, i64)> = Vec::new();
    for t in transactions.iter().filter(|t| !t.income) {
        match totals.iter_mut().find(|(c, _)| *c == t.category) {
            Some((_, sum)) => *sum += t.cents,
            None => totals.push((t.category.clone(), t.cents)),
        }
    }
    totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    totals
}

pub fn csv_lines(transactions: &[Transaction]) -> Vec<Vec<String>> {
    let mut rows = vec![vec!["date".into(), "amount".into(), "category".into(), "note".into()]];
    for t in transactions {
        let sign = if t.income { "" } else { "-" };
        rows.push(vec![t.day.to_string(), format!("{sign}{}.{:02}", t.cents / 100, t.cents % 100), t.category.clone(), t.note.clone()]);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_in_any_writing() {
        for (text, cents) in [("12,50", 1250), ("12.5", 1250), ("1.234,56", 123456), ("1,234.56", 123456), ("€ 3", 300), ("0,05", 5), ("1.234", 123400)] {
            assert_eq!(parse_cents(text), Ok(cents), "{text}");
        }
        for text in ["", "abc", ",", "0,00x"] {
            assert!(parse_cents(text).is_err(), "{text}");
        }
    }
}
