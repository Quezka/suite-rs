//! Italian employee payslips: gross pay, INPS, IRPEF with detrazioni, tredicesima and TFR (port of
//! `quire/domain/payroll.py`, mirrored by the phone's `Payroll.kt`; all three must give the same cents).
//!
//! Pure rules. Figures are estimates: the real payslip also depends on other income, family detrazioni and local
//! rules. A year is worked out as a whole, then spread over its months by taxable pay, like the employer's year-end
//! conguaglio does.

use chrono::{Datelike, NaiveDate};

/// IRPEF 2026: (ceiling, rate).
pub const BRACKETS: [(f64, f64); 3] = [(28_000.0, 0.23), (50_000.0, 0.33), (f64::INFINITY, 0.43)];
pub const TFR_DIVISOR: f64 = 13.5;
/// Withheld from the TFR quota for the INPS guarantee fund.
pub const TFR_FUND_SHARE: f64 = 0.005;
/// Cuneo fiscale bonus (income-dependent share of taxable pay), up to a 20,000 income.
pub const CUNEO: [(f64, f64); 3] = [(8_500.0, 0.071), (15_000.0, 0.053), (20_000.0, 0.048)];

/// The payroll's rounding: Python's `round(value + 1e-9, 2)` (a hair up, so a computed .xx5 doesn't fall short).
pub fn round2(value: f64) -> f64 {
    super::model::round2(value + 1e-9)
}

/// What a job pays and how it's taxed (the pay part of a Job).
#[derive(Clone, Debug, PartialEq)]
pub struct PayTerms {
    pub pay_mode: String,
    /// Gross per mensilità.
    pub monthly_pay: Option<f64>,
    /// 12, 13 or 14.
    pub mensilities: u8,
    pub contract_start: Option<NaiveDate>,
    pub contract_end: Option<NaiveDate>,
    pub tax_model: String,
    /// Flat model: percent withheld.
    pub deductions: f64,
    /// Italy model: employee contributions, percent.
    pub inps: f64,
    /// Italy model: regional + municipal surtax, percent.
    pub addizionali: f64,
    pub fixed_term: bool,
    pub cuneo: bool,
}

impl Default for PayTerms {
    fn default() -> Self {
        Self {
            pay_mode: "hourly".into(),
            monthly_pay: None,
            mensilities: 13,
            contract_start: None,
            contract_end: None,
            tax_model: "flat".into(),
            deductions: 0.0,
            inps: 9.19,
            addizionali: 0.0,
            fixed_term: true,
            cuneo: false,
        }
    }
}

impl PayTerms {
    /// Take-home share before any income tax: what's left after the contributions.
    pub fn default_ratio(&self) -> f64 {
        let rate = if self.tax_model == "italy" { self.inps } else { self.deductions };
        1.0 - rate / 100.0
    }
}

impl super::model::Job {
    pub fn terms(&self) -> PayTerms {
        PayTerms {
            pay_mode: self.pay_mode.clone(),
            monthly_pay: self.monthly_pay,
            mensilities: self.mensilities,
            contract_start: self.contract_start,
            contract_end: self.contract_end,
            tax_model: self.tax_model.clone(),
            deductions: self.deductions,
            inps: self.inps,
            addizionali: self.addizionali,
            fixed_term: self.fixed_term,
            cuneo: self.cuneo,
        }
    }
}

/// One calendar month of one job.
#[derive(Clone, Debug, PartialEq)]
pub struct Payslip {
    pub year: i32,
    pub month: u32,
    pub gross: f64,
    /// The part of gross that isn't tredicesima/quattordicesima.
    pub regular: f64,
    /// INPS, or the flat percentage.
    pub contributions: f64,
    pub irpef: f64,
    pub addizionali: f64,
    pub bonus: f64,
    /// Set aside for the severance pay, not part of net.
    pub tfr: f64,
}

impl Payslip {
    pub fn net(&self) -> f64 {
        round2(self.gross - self.contributions - self.irpef - self.addizionali + self.bonus)
    }

    pub fn extra(&self) -> f64 {
        round2(self.gross - self.regular)
    }

    /// Share of gross that is take-home, or None for an unpaid month.
    pub fn ratio(&self) -> Option<f64> {
        (self.gross > 0.0).then(|| self.net() / self.gross)
    }
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
}

/// Paydays from `since` to `today`, each with the (year, month) it pays, oldest first. A payday past the month's
/// end falls on its last day. Mirrors the Python app's `due_paydays`.
pub fn due_paydays(payday: Option<u32>, pays_previous_month: bool, since: Option<NaiveDate>, today: NaiveDate)
    -> Vec<(NaiveDate, i32, u32)> {
    let (Some(payday), Some(since)) = (payday, since) else { return Vec::new() };
    let mut out = Vec::new();
    let (mut year, mut month) = (since.year(), since.month());
    while (year, month) <= (today.year(), today.month()) {
        let day = NaiveDate::from_ymd_opt(year, month, payday.min(month_days(year, month))).expect("a real day");
        if since <= day && day <= today {
            let paid = if !pays_previous_month { (year, month) } else if month > 1 { (year, month - 1) } else { (year - 1, 12) };
            out.push((day, paid.0, paid.1));
        }
        (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    }
    out
}

#[cfg(test)]
mod payday_tests {
    use super::*;
    fn d(y: i32, m: u32, day: u32) -> NaiveDate { NaiveDate::from_ymd_opt(y, m, day).unwrap() }

    #[test]
    fn same_answers_as_python() {
        assert!(due_paydays(None, true, Some(d(2026, 1, 1)), d(2026, 6, 1)).is_empty());
        assert_eq!(due_paydays(Some(31), true, Some(d(2026, 1, 5)), d(2026, 3, 30)),
                   vec![(d(2026, 1, 31), 2025, 12), (d(2026, 2, 28), 2026, 1)]);
        assert_eq!(due_paydays(Some(10), false, Some(d(2026, 1, 11)), d(2026, 2, 10)), vec![(d(2026, 2, 10), 2026, 2)]);
    }
}

pub fn month_days(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    (date(ny, nm, 1) - date(year, month, 1)).num_days() as u32
}

fn days_between(start: NaiveDate, end: NaiveDate) -> i64 {
    ((end - start).num_days() + 1).max(0)
}

/// Days of the month inside the contract (the whole month when it has no dates).
pub fn contract_days(terms: &PayTerms, year: i32, month: u32) -> i64 {
    let (first, last) = (date(year, month, 1), date(year, month, month_days(year, month)));
    let start = terms.contract_start.map_or(first, |s| s.max(first));
    let end = terms.contract_end.map_or(last, |e| e.min(last));
    days_between(start, end)
}

/// A month counts towards tredicesima when at least 15 days of it are worked.
pub fn employed(terms: &PayTerms, year: i32, month: u32) -> bool {
    contract_days(terms, year, month) >= 15
}

fn monthly_regular(terms: &PayTerms, year: i32, month: u32) -> f64 {
    let Some(pay) = terms.monthly_pay else { return 0.0 };
    round2(pay * contract_days(terms, year, month) as f64 / f64::from(month_days(year, month)))
}

fn months_between(terms: &PayTerms, start: NaiveDate, end_year: i32, end_month: u32) -> u32 {
    let (mut count, mut year, mut month) = (0, start.year(), start.month());
    while (year, month) <= (end_year, end_month) {
        count += u32::from(employed(terms, year, month));
        (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    }
    count
}

/// Tredicesima (paid in December) and quattordicesima (paid in June), pro rata. When the contract ends, whatever has
/// built up is paid with the last payslip.
fn extra_pay(terms: &PayTerms, year: i32, month: u32) -> f64 {
    let Some(pay) = terms.monthly_pay else { return 0.0 };
    if terms.mensilities <= 12 {
        return 0.0;
    }
    let here = (year, month);
    if terms.contract_end.is_some_and(|e| here > (e.year(), e.month())) {
        return 0.0;
    }
    if terms.contract_start.is_some_and(|s| here < (s.year(), s.month())) {
        return 0.0;
    }
    let ends = terms.contract_end.is_some_and(|e| (e.year(), e.month()) == here);
    let mut total = 0.0;
    if month == 12 || ends {
        total += pay * f64::from(months_between(terms, date(year, 1, 1), year, month)) / 12.0;
    }
    if terms.mensilities >= 14 && (month == 6 || ends) {
        let start = if month <= 6 { date(year - 1, 7, 1) } else { date(year, 7, 1) };
        total += pay * f64::from(months_between(terms, start, year, month)) / 12.0;
    }
    round2(total)
}

/// (regular, extra) gross of a month for a job paid by the month.
pub fn monthly_gross(terms: &PayTerms, year: i32, month: u32) -> (f64, f64) {
    (monthly_regular(terms, year, month), extra_pay(terms, year, month))
}

/// Days of the year the job lasted: what the work detrazione is prorated by.
pub fn employment_days(terms: &PayTerms, year: i32, worked: &[NaiveDate]) -> i64 {
    let (first, last) = (date(year, 1, 1), date(year, 12, 31));
    let (start, end) = if terms.contract_start.is_some() || terms.contract_end.is_some() {
        (terms.contract_start.map_or(first, |s| s.max(first)), terms.contract_end.map_or(last, |e| e.min(last)))
    } else if terms.pay_mode == "hourly" {
        let mut days: Vec<NaiveDate> = worked.iter().copied().filter(|d| d.year() == year).collect();
        days.sort();
        match (days.first(), days.last()) {
            (Some(a), Some(b)) => (*a, *b),
            _ => return 0,
        }
    } else {
        (first, last)
    };
    days_between(start, end)
}

pub fn irpef_before_detrazioni(taxable: f64) -> f64 {
    let (mut tax, mut floor) = (0.0, 0.0);
    for (ceiling, rate) in BRACKETS {
        if taxable > floor {
            tax += (taxable.min(ceiling) - floor) * rate;
        }
        floor = ceiling;
    }
    tax
}

/// Detrazione for employees, prorated by the days worked in the year.
pub fn work_detrazione(taxable: f64, days: i64, fixed_term: bool) -> f64 {
    if taxable <= 0.0 || days <= 0 {
        return 0.0;
    }
    let mut full = if taxable <= 15_000.0 {
        1_955.0
    } else if taxable <= 28_000.0 {
        1_910.0 + 1_190.0 * (28_000.0 - taxable) / 13_000.0
    } else if taxable <= 50_000.0 {
        1_910.0 * (50_000.0 - taxable) / 22_000.0
    } else {
        0.0
    };
    if 25_000.0 < taxable && taxable <= 35_000.0 {
        full += 65.0;
    }
    if taxable <= 15_000.0 {
        full = full.max(if fixed_term { 690.0 } else { 1_380.0 });
    }
    full * days.min(365) as f64 / 365.0
}

pub fn cuneo_rate(taxable: f64) -> f64 {
    CUNEO.iter().find(|(ceiling, _)| taxable <= *ceiling).map_or(0.0, |(_, rate)| *rate)
}

/// Split `total` in proportion to `weights`, to the cent, summing back exactly.
fn spread(total: f64, weights: &[f64]) -> Vec<f64> {
    let whole: f64 = weights.iter().sum();
    if total == 0.0 || whole <= 0.0 {
        return vec![0.0; weights.len()];
    }
    let mut parts: Vec<f64> = weights.iter().map(|w| round2(total * w / whole)).collect();
    if let Some(last) = weights.iter().rposition(|w| *w > 0.0) {
        let sum: f64 = parts.iter().sum();
        parts[last] = round2(parts[last] + total - sum);
    }
    parts
}

/// The twelve payslips of `year`. `grosses` is (regular, extra) gross pay for each month: hours times the rate for an
/// hourly job, `monthly_gross()` for a monthly one. `worked` are the days with shifts, used to date an hourly job
/// that has no contract dates.
pub fn year_payslips(terms: &PayTerms, year: i32, grosses: &[(f64, f64); 12], worked: &[NaiveDate]) -> Vec<Payslip> {
    let gross: Vec<f64> = grosses.iter().map(|(r, e)| round2(r + e)).collect();
    let rate = if terms.tax_model == "italy" { terms.inps } else { terms.deductions };
    let contributions: Vec<f64> = gross.iter().map(|g| round2(g * rate / 100.0)).collect();
    let mut irpef = vec![0.0; 12];
    let mut addizionali = vec![0.0; 12];
    let mut bonus = vec![0.0; 12];
    if terms.tax_model == "italy" && gross.iter().sum::<f64>() > 0.0 {
        let taxable: Vec<f64> = gross.iter().zip(&contributions).map(|(g, c)| g - c).collect();
        let income: f64 = taxable.iter().sum();
        let days = employment_days(terms, year, worked);
        let due = round2((irpef_before_detrazioni(income) - work_detrazione(income, days, terms.fixed_term)).max(0.0));
        irpef = spread(due, &taxable);
        if due > 0.0 {
            addizionali = spread(round2(income * terms.addizionali / 100.0), &taxable);
        }
        if terms.cuneo {
            bonus = spread(round2(income * cuneo_rate(income)), &taxable);
        }
    }
    (0..12)
        .map(|m| Payslip {
            year,
            month: m as u32 + 1,
            gross: gross[m],
            regular: round2(grosses[m].0),
            contributions: contributions[m],
            irpef: irpef[m],
            addizionali: addizionali[m],
            bonus: bonus[m],
            tfr: round2(gross[m] / TFR_DIVISOR - gross[m] * TFR_FUND_SHARE),
        })
        .collect()
}

/// Net of `gross` earned in the month of `slip`, at that month's take-home share.
pub fn take_home(terms: &PayTerms, slip: &Payslip, gross: f64) -> f64 {
    round2(gross * slip.ratio().unwrap_or_else(|| terms.default_ratio()))
}

/// (gross, net) of a monthly job's payslip that falls inside `first..=last`. The regular pay counts by the contract
/// days in the range; tredicesima and quattordicesima count when the range holds the month's last contract day.
pub fn monthly_share(terms: &PayTerms, slip: &Payslip, first: NaiveDate, last: NaiveDate) -> (f64, f64) {
    let (year, month) = (slip.year, slip.month);
    let mut start = date(year, month, 1);
    let mut end = date(year, month, month_days(year, month));
    if let Some(s) = terms.contract_start {
        start = start.max(s);
    }
    if let Some(e) = terms.contract_end {
        end = end.min(e);
    }
    if start > end || slip.gross <= 0.0 {
        return (0.0, 0.0);
    }
    let (lo, hi) = (start.max(first), end.min(last));
    let days = days_between(lo, hi);
    let mut gross = slip.regular * days as f64 / days_between(start, end) as f64;
    if first <= end && end <= last {
        gross += slip.extra();
    }
    if gross <= 0.0 {
        return (0.0, 0.0);
    }
    (round2(gross), round2(slip.net() * gross / slip.gross))
}
