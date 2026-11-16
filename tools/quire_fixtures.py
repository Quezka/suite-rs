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
    from quire.domain import water, sleep, register, medicine, ical
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
    entries = [ical.CalendarEntry("e1@quire", "Maths; test, chapter 4", date(2026, 3, 10), 510, 600, False, "Bring the calculator\nand a pencil", "Room B1"),
               ical.CalendarEntry("e2@quire", "Gita a Firenze — " + "очень длинное название " * 4, date(2026, 3, 11), all_day=True)]
    out["ics"] = ical.to_ics(entries, "Quire, agenda", datetime(2026, 3, 9, 15, 30))
    bodies = ["# Mitochondria\n\nThe **powerhouse** of the cell\n- [ ] see [p. 4](http://x)", "Only a title", "T\n*Nature vs. nurture* and _this_",
              "## Lista\n- [x] uno\n- [ ] due\n> citazione `code` ~~via~~\n### Sotto\ntesto " + "lungo " * 30,
              "\n\n   \nPrima riga\n1. punto\n* stella\n+ più\nsnake_case_name and a*b*c"]
    out["notes"] = [[b, derive_note_title(b), note_snippet(b), note_snippet(b, 20)] for b in bodies]
    return out


def storage_fixtures():
    """A demo database made by the Python app, what it sends, and the tables after applying the phone's records."""
    import os
    import sqlite3
    from quire.bootstrap import build_services
    from quire.demo import seed, seed_school
    from quire.infrastructure.sqlite import SqliteDatabase
    from quire.infrastructure.sync_store import SqliteSyncStore, SYNCED
    from quire.application.ports import SyncRecord
    db_path = OUT / "python.db"
    for extra in ("", "-wal", "-shm"):
        if os.path.exists(str(db_path) + extra):
            os.remove(str(db_path) + extra)
    from quire.infrastructure.demo_register import DemoRegister
    from quire.infrastructure.credentials import MemoryCredentialStore
    services, db = build_services(str(db_path), register=DemoRegister(date(2026, 11, 9)),
                                  credentials=MemoryCredentialStore())
    seed(services)
    seed_school(services)
    store = SqliteSyncStore(db)
    out = [{"kind": r.kind, "uid": r.uid, "modified": r.modified, "deleted": r.deleted, "data": r.data} for r in store.outgoing()]
    schema = sorted(row[0] for row in db.conn.execute("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL"))
    db.conn.commit()
    db.close()
    records = json.loads((HERE.parent / "quire/android/app/src/test/resources/desktop_records.json").read_text())
    fresh = SqliteDatabase(":memory:")
    SqliteSyncStore(fresh).apply([SyncRecord(r["kind"], r["uid"], r["modified"], r["deleted"], r["data"]) for r in records])
    tables = {}
    for s in SYNCED:
        rows = fresh.conn.execute(f"SELECT * FROM {s.table} ORDER BY rowid").fetchall()
        tables[s.table] = [[(x.hex() if isinstance(x, bytes) else x) for x in tuple(r)] for r in rows]
    for child in ("class_slots", "shift_patterns", "shift_skips"):
        tables[child] = [list(r) for r in fresh.conn.execute(f"SELECT * FROM {child} ORDER BY rowid")]
    tables["sync_inbox"] = [list(r) for r in fresh.conn.execute("SELECT kind, uid FROM sync_inbox ORDER BY kind, uid")]
    return {"outgoing": out, "schema": schema, "applied": tables, "records": records}


def agenda_fixtures():
    """Day agendas the Python planner makes from python.db (a copy), for days around the demo, with a fixed today."""
    import shutil
    import tempfile
    from datetime import datetime, timedelta
    from quire.bootstrap import build_services
    from quire.infrastructure.credentials import MemoryCredentialStore
    copy = Path(tempfile.mkdtemp()) / "agenda.db"
    shutil.copy(OUT / "python.db", copy)
    import sqlite3
    first = sqlite3.connect(copy).execute("SELECT MIN(day) FROM events").fetchone()[0]
    today = date.fromisoformat(first)

    class Fixed:
        def now(self):
            return datetime.combine(today, datetime.min.time()).replace(hour=12)

        def today(self):
            return today

    services, db = build_services(str(copy), clock=Fixed(), credentials=MemoryCredentialStore())
    days = []
    for n in range(-2, 9):
        day = today + timedelta(days=n)
        a = services.planner.day_agenda(day)
        days.append({"day": day.isoformat(), "is_today": a.is_today,
                     "items": [[i.kind.value, i.title, i.time.start, i.time.end, i.room, i.details] for i in a.items],
                     "tasks": [[t.task.title, t.overdue] for t in a.tasks], "journal": a.journal})
    digests = []
    for morning in (True, False):
        g = services.digest.build(morning, today, mark=False)
        digests.append({"morning": morning, "items": [list(i) for i in g.items], "due": list(g.due),
                        "overdue": g.overdue, "ideas": list(g.ideas), "idea_back": g.idea_back})
    notes = [[n.title, n.pinned, n.topic, n.snippet, n.course.name if n.course else None,
              n.notebook.name if n.notebook else None] for n in services.notes.search()]
    groups = [[g.course.name if g.course else None, g.notebook.name if g.notebook else None, g.topic, len(g.notes)]
              for g in services.notes.grouped()]
    from quire.application.services.notes import class_note_title
    work = []
    for first, last in ((today - timedelta(days=today.weekday()), today - timedelta(days=today.weekday()) + timedelta(days=6)),
                        (today.replace(day=1), today.replace(day=28))):
        w = services.work.summary(first, last)
        work.append({"first": first.isoformat(), "last": last.isoformat(), "minutes": w.minutes, "pay": w.pay, "net": w.net,
                     "shifts": w.shifts,
                     "per_job": [[t.job.name if t.job else None, t.minutes, t.pay, t.net] for t in w.per_job]})
        for i in services.work.shifts_between(first, last):
            work[-1].setdefault("items", []).append([i.shift.day.isoformat(), i.shift.start, i.shift.duration, i.pay, i.net])
    r = services.school
    o = r.overview()
    a = r.absence_summary()
    school = {
        "periods": r.periods(),
        "subjects": [[x.subject, x.average, len(x.grades), x.course.name if x.course else None] for x in r.grades_by_subject()],
        "average": r.overall_average(),
        "overview": [o.average, o.grade_count, o.next_test.task.title if o.next_test else None, o.homework_due_this_week,
                     o.next_homework.task.title if o.next_homework else None, [x.subject for x in o.below_pass]],
        "agenda": [[i.task.title, i.overdue] for i in r.agenda(include_done=True)],
        "absences": [a.absent_days, a.late_entries, a.early_exits, a.unjustified, a.hours_missed, a.school_hours, a.limit_hours],
        "unread": r.unread_notices(),
    }
    subjects = r.grades_by_subject()
    if subjects:
        n = r.needed(subjects[0].subject, 7.0, 2)
        school["needed"] = [n.subject, n.mark, n.average, n.grade_count]
        school["trend"] = [[d.isoformat(), v] for d, v in r.trend()]
    from quire.domain.ical import to_ics
    ics = to_ics(services.calendar._entries(), "Quire", Fixed().now())
    db.close()
    return {"today": today.isoformat(), "days": days, "digests": digests, "school": school, "ics": ics, "notes": notes, "note_groups": groups,
            "class_note_title": class_note_title("Biology", date(2026, 11, 16)), "work": work}


OUT.mkdir(parents=True, exist_ok=True)
(OUT / "storage.json").write_text(json.dumps(storage_fixtures(), indent=1, ensure_ascii=False))
(OUT / "agenda.json").write_text(json.dumps(agenda_fixtures(), indent=1, ensure_ascii=False))
(OUT / "rules.json").write_text(json.dumps(rules_cases(), indent=1, ensure_ascii=False))
(OUT / "payroll.json").write_text(json.dumps(payroll_cases(), indent=1))
print("wrote", OUT)
