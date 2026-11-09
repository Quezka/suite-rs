//! The Rust payroll gives the same cents as the Python one (fixtures from `tools/quire_fixtures.py`).

use chrono::NaiveDate;
use quire::domain::payroll::*;
use serde_json::Value;

fn date(v: &Value) -> Option<NaiveDate> {
    v.as_str().map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap())
}

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 1e-9, "{what}: rust {a} python {b}");
}

#[test]
fn payslips_match_python() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/payroll.json")).unwrap();
    for (n, case) in cases.iter().enumerate() {
        let t = &case["terms"];
        let terms = PayTerms {
            pay_mode: t["pay_mode"].as_str().unwrap().into(),
            monthly_pay: t["monthly_pay"].as_f64(),
            mensilities: t["mensilities"].as_u64().unwrap() as u8,
            contract_start: date(&t["contract_start"]),
            contract_end: date(&t["contract_end"]),
            tax_model: t["tax_model"].as_str().unwrap().into(),
            deductions: t["deductions"].as_f64().unwrap(),
            inps: t["inps"].as_f64().unwrap(),
            addizionali: t["addizionali"].as_f64().unwrap(),
            fixed_term: t["fixed_term"].as_bool().unwrap(),
            cuneo: t["cuneo"].as_bool().unwrap(),
        };
        let grosses: Vec<(f64, f64)> =
            case["grosses"].as_array().unwrap().iter().map(|g| (g[0].as_f64().unwrap(), g[1].as_f64().unwrap())).collect();
        if terms.pay_mode == "monthly" {
            for (m, g) in grosses.iter().enumerate() {
                let ours = monthly_gross(&terms, 2026, m as u32 + 1);
                close(ours.0, g.0, &format!("case {n} month {} regular", m + 1));
                close(ours.1, g.1, &format!("case {n} month {} extra", m + 1));
            }
        }
        let worked: Vec<NaiveDate> = case["worked"].as_array().unwrap().iter().filter_map(date).collect();
        let slips = year_payslips(&terms, 2026, &grosses.try_into().unwrap(), &worked);
        for (slip, want) in slips.iter().zip(case["slips"].as_array().unwrap()) {
            let at = format!("case {n} month {}", slip.month);
            for (ours, field) in [
                (slip.gross, "gross"),
                (slip.regular, "regular"),
                (slip.contributions, "contributions"),
                (slip.irpef, "irpef"),
                (slip.addizionali, "addizionali"),
                (slip.bonus, "bonus"),
                (slip.tfr, "tfr"),
                (slip.net(), "net"),
                (slip.extra(), "extra"),
            ] {
                close(ours, want[field].as_f64().unwrap(), &format!("{at} {field}"));
            }
        }
        for (slip, want) in slips.iter().zip(case["shares"].as_array().unwrap()) {
            let (g, net) = monthly_share(&terms, slip, NaiveDate::from_ymd_opt(2026, slip.month, 10).unwrap(), NaiveDate::from_ymd_opt(2026, slip.month, 25).unwrap());
            close(g, want[0].as_f64().unwrap(), &format!("case {n} share gross {}", slip.month));
            close(net, want[1].as_f64().unwrap(), &format!("case {n} share net {}", slip.month));
        }
        for (slip, want) in slips.iter().zip(case["take_home"].as_array().unwrap()) {
            close(take_home(&terms, slip, 123.45), want.as_f64().unwrap(), &format!("case {n} take home {}", slip.month));
        }
    }
}
