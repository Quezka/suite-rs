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


OUT.mkdir(parents=True, exist_ok=True)
(OUT / "payroll.json").write_text(json.dumps(payroll_cases(), indent=1))
print("wrote", OUT)
