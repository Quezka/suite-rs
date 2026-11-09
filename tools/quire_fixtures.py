"""Writes crates/quire/tests/fixtures/*.json from the Python Quire (../quire): answers the Rust port must give too.

    python3 tools/quire_fixtures.py        (from suite-rs; needs ../quire next to it)
"""
import json
import sys
from datetime import date
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE.parent / "quire"))

from quire.domain import payroll as p  # noqa: E402

OUT = HERE / "crates" / "quire" / "tests" / "fixtures"


def d(x):
    return x.isoformat() if x else None


def payroll_cases():
    cases = []
    hourly = [(0.0, 0.0)] * 12
    hourly = [(round(37 * 9.5 * (1 + m % 3), 2), 0.0) for m in range(12)]
    worked = [date(2026, m, 3) for m in range(2, 12)]
    terms_list = [
        p.PayTerms("monthly", 1650.0, 13, date(2026, 3, 16), date(2026, 10, 10), "italy", 0.0, 9.19, 2.03, True, True),
        p.PayTerms("monthly", 2400.0, 14, None, None, "italy", 0.0, 9.19, 1.5, False, False),
        p.PayTerms("monthly", 900.0, 13, date(2025, 9, 1), None, "italy", 0.0, 9.19, 0.0, True, True),
        p.PayTerms("hourly", None, 13, None, None, "italy", 0.0, 9.19, 1.23, True, True),
        p.PayTerms("hourly", None, 13, None, None, "flat", 21.5, 9.19, 0.0, True, False),
        p.PayTerms("monthly", 3900.0, 12, date(2026, 1, 1), date(2026, 12, 31), "italy", 0.0, 9.19, 3.33, True, False),
    ]
    for terms in terms_list:
        if terms.pay_mode == "monthly":
            grosses = [p.monthly_gross(terms, 2026, m) for m in range(1, 13)]
        else:
            grosses = hourly
        slips = p.year_payslips(terms, 2026, grosses, worked)
        shares = [p.monthly_share(terms, s, date(2026, s.month, 10), date(2026, s.month, 25)) for s in slips]
        cases.append({
            "terms": {k: (d(v) if isinstance(v, date) else v) for k, v in terms.__dict__.items()},
            "grosses": grosses,
            "worked": [d(x) for x in worked],
            "slips": [{**s.__dict__, "net": s.net, "extra": s.extra} for s in slips],
            "shares": shares,
            "take_home": [p.take_home(terms, s, 123.45) for s in slips],
        })
    return cases


def rules_cases():
    from datetime import datetime
    from quire.domain import water, sleep, register, finance, medicine, ical
    from quire.domain.model import note_snippet, derive_note_title
    out = {}
    out["water_goal"] = [[h, w, water.water_goal_ml(h, w)] for h in (150, 162.5, 175, 190, 201) for w in (45, 58.3, 70, 95, 140)]
    out["expected"] = [[g, m, water.expected_ml(g, m)] for g in (1500, 2250, 3100) for m in (400, 481, 600, 777, 900, 1100, 1320, 1400)]
    out["reminder"] = [[g, dr, m, last, water.reminder_due(g, dr, m, last)] for g, dr in ((2200, 0), (2200, 900), (2200, 2300)) for m in (450, 600, 1000, 1300) for last in (None, m - 30, m - 120)]
    logs = [sleep.SleepLog(date(2026, 3, d), a, w, q) for d, a, w, q in ((1, 23*60+10, 7*60, 4), (2, 30, 7*60+30, None), (3, 22*60, 6*60, 5), (4, 23*60+45, 6*60+30, 2), (6, 1*60, 8*60, 3), (7, 22*60+30, 6*60+45, None))]
    def stats(ls, g):
        s = sleep.sleep_stats(ls, g)
        return {**s.__dict__}
    out["sleep"] = [{"logs": [[l.day.isoformat(), l.asleep, l.woke, l.quality] for l in ls], "goal": g, "stats": stats(ls, g)} for ls in (logs, logs[:3], logs[2:]) for g in (7.5, 8.0, 8.25)]
    out["bedtime"] = [[w, h, sleep.bedtime_for(w, h)] for w in (0, 360, 420, 450) for h in (6.5, 7.75, 8.0, 9.125)]
    marks = [6.5, 7.25, 5.5, 8.0, 4.75]
    out["needed"] = [[marks[:k], t, n, register.needed_grade(marks[:k], t, n)] for k in (0, 1, 3, 5) for t in (6.0, 7.0, 8.5) for n in (1, 2, 3)]
    pts = [(date(2026, 1, d), v) for d, v in ((3, 6.5), (3, 7.0), (5, 5.25), (9, 8.75), (12, 7.5), (12, 4.0))]
    out["running"] = [[[d.isoformat(), v] for d, v in pts], [[d.isoformat(), v] for d, v in register.running_average(pts)]]
    out["lesson_hours"] = [[m, register.lesson_hours(m)] for m in (0, 30, 50, 89, 90, 91, 110, 150, 151)]
    out["cents"] = [[t, finance.parse_cents(t)] for t in ("12,50", "12.5", "1.234,56", "1,234.56", "€ 3", "0,05", "1.234", "7", "10.000,5", "£1,2")]
    entries = [ical.CalendarEntry("e1@quire", "Maths; test, chapter 4", date(2026, 3, 10), 510, 600, False, "Bring the calculator\nand a pencil", "Room B1"),
               ical.CalendarEntry("e2@quire", "Gita a Firenze — " + "очень длинное название " * 4, date(2026, 3, 11), all_day=True)]
    out["ics"] = ical.to_ics(entries, "Quire, agenda", datetime(2026, 3, 9, 15, 30))
    bodies = ["# Mitochondria\n\nThe **powerhouse** of the cell\n- [ ] see [p. 4](http://x)", "Only a title", "T\n*Nature vs. nurture* and _this_",
              "## Lista\n- [x] uno\n- [ ] due\n> citazione `code` ~~via~~\n### Sotto\ntesto " + "lungo " * 30,
              "\n\n   \nPrima riga\n1. punto\n* stella\n+ più\nsnake_case_name and a*b*c"]
    out["notes"] = [[b, derive_note_title(b), note_snippet(b), note_snippet(b, 20)] for b in bodies]
    return out


OUT.mkdir(parents=True, exist_ok=True)
(OUT / "rules.json").write_text(json.dumps(rules_cases(), indent=1, ensure_ascii=False))
(OUT / "payroll.json").write_text(json.dumps(payroll_cases(), indent=1))
print("wrote", OUT)
