//! today: day agenda, the one thing now, due tasks + quick add, day notes (= python `views/today.py`, list not grid yet)

use chrono::{Duration, NaiveDate, Timelike};
use egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use quezka_kit::fonts::Weight;
use quezka_kit::forms::{self as f, WidthClass};
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::Tokens;

use super::common::{page_title, Notice};
use super::format::{fmt_min, fmt_range, long_date, relative_date, was_due};
use super::i18n::{t, tf};
use crate::application::planner::{AgendaItem, DayAgenda};
use crate::application::services::Services;
use crate::application::tasks::TaskInput;
use crate::domain::model::Task;

/// item on at `minute` + next to start (= python `now_next`)
pub fn now_next(items: &[AgendaItem], minute: i32) -> (Option<&AgendaItem>, Option<&AgendaItem>) {
    let mut timed: Vec<&AgendaItem> = items.iter().filter(|i| i.time.end > i.time.start).collect();
    timed.sort_by_key(|i| i.time.start);
    let now = timed.iter().rev().find(|i| i.time.start <= minute && minute < i.time.end).copied();
    let later = timed.iter().find(|i| i.time.start > minute && now.is_none_or(|n| !std::ptr::eq(**i, n))).copied();
    (now, later)
}

struct Loaded {
    count: u64,
    day: NaiveDate,
    minute: i32,
    agenda: DayAgenda,
    step: Option<Task>,
}

pub struct TodayPage {
    pub day: NaiveDate,
    loaded: Option<Loaded>,
    quick: String,
    journal: String,
    journal_day: Option<NaiveDate>,
    /// time of last keystroke; saved `0.6 s` after
    journal_edit: Option<f64>,
    picking: bool,
    notice: Notice,
}

impl TodayPage {
    pub fn new(today: NaiveDate) -> Self {
        Self { day: today, loaded: None, quick: String::new(), journal: String::new(), journal_day: None, journal_edit: None,
            picking: false, notice: Notice::default() }
    }

    fn go(&mut self, s: &Services, day: NaiveDate) {
        self.flush_journal(s);
        self.day = day;
    }

    pub fn flush_journal(&mut self, s: &Services) {
        if let (Some(_), Some(day)) = (self.journal_edit.take(), self.journal_day) {
            if let Err(e) = s.planner.save_journal(day, &self.journal) {
                self.notice.fail(t(&e.to_string()));
            }
        }
    }

    fn load(&mut self, s: &Services, count: u64) {
        let now = s.planner.clock.now();
        let minute = (now.hour() * 60 + now.minute()) as i32;
        let fresh = self.loaded.as_ref().is_some_and(|l| l.count == count && l.day == self.day && l.minute == minute);
        if fresh {
            return;
        }
        match s.planner.day_agenda(self.day) {
            Ok(agenda) => {
                if self.journal_day != Some(self.day) {
                    self.journal = agenda.journal.clone();
                    self.journal_day = Some(self.day);
                }
                let step = s.tasks.next_step().ok().flatten();
                self.loaded = Some(Loaded { count, day: self.day, minute, agenda, step });
            }
            Err(e) => self.notice.fail(t(&e.to_string())),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, th: &Tokens, s: &Services, count: u64, width: WidthClass) {
        self.load(s, count);
        let today = s.planner.today();
        if self.journal_edit.is_some_and(|at| ui.input(|i| i.time) - at > 0.6) {
            self.flush_journal(s);
        }
        if self.journal_edit.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(30)); // now card follows clock

        self.header(ui, th, s, today);
        self.notice.ui(ui, th);
        ui.add_space(8.0);
        let Some(loaded) = self.loaded.take() else { return };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if width == WidthClass::Expanded {
                ui.columns(2, |c| {
                    self.agenda_card(&mut c[0], th, &loaded.agenda);
                    if self.day == today {
                        self.now_card(&mut c[1], th, s, &loaded);
                    }
                    self.due_card(&mut c[1], th, s, &loaded.agenda, today);
                    self.notes_card(&mut c[1], th);
                });
            } else {
                if self.day == today {
                    self.now_card(ui, th, s, &loaded);
                }
                self.agenda_card(ui, th, &loaded.agenda);
                self.due_card(ui, th, s, &loaded.agenda, today);
                self.notes_card(ui, th);
            }
        });
        if self.loaded.is_none() {
            self.loaded = Some(loaded);
        }
    }

    fn header(&mut self, ui: &mut Ui, th: &Tokens, s: &Services, today: NaiveDate) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| page_title(ui, th, &long_date(self.day), &relative_date(self.day, today)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::icon_button(ui, th, "chevron-right", th.text) {
                    self.go(s, self.day + Duration::days(1));
                }
                if self.day != today && f::inline_button(ui, th, &t("Today"), Kind::Tonal) {
                    self.go(s, today);
                }
                if w::icon_button(ui, th, "calendar-day", th.text) {
                    self.picking = true;
                }
                if w::icon_button(ui, th, "chevron-left", th.text) {
                    self.go(s, self.day - Duration::days(1));
                }
            });
        });
        if self.picking {
            let mut day = self.day;
            let mut chosen = false;
            let weekdays = super::i18n::names().weekdays_short.map(String::from);
            let out = f::dialog(ui.ctx(), th, ui.scope_id().with("pick-day"), &t("Pick a date"), |ui| {
                chosen = f::date_picker(ui, th, ui.scope_id().with("cal"), &mut day, today, &|m| super::i18n::month_name(m).to_string(), &weekdays);
            });
            if chosen {
                self.go(s, day);
                self.picking = false;
            }
            if out.dismissed {
                self.picking = false;
            }
        }
    }

    fn now_card(&mut self, ui: &mut Ui, th: &Tokens, s: &Services, l: &Loaded) {
        let (now, later) = now_next(&l.agenda.items, l.minute);
        w::hero(ui, th, |ui| {
            let kicker = if now.is_some() { t("Now") } else { t("The one thing now") };
            ui.label(w::text(kicker, 14.0, th.on_accent_soft, Weight::SemiBold));
            let (headline, detail) = match (now, &l.step, later) {
                (Some(n), _, _) => (n.title.clone(), tf("until {time}", &[("time", &fmt_min(n.time.end))])),
                (None, Some(step), later) => (step.title.clone(),
                    later.map(|x| tf("Next: {what} at {time}", &[("what", &x.title), ("time", &fmt_min(x.time.start))])).unwrap_or_default()),
                (None, None, Some(x)) => (x.title.clone(), tf("at {time}", &[("time", &fmt_min(x.time.start))])),
                _ => (t("Nothing to do right now. Breathe."), String::new()),
            };
            ui.label(w::text(headline, 22.0, th.text, Weight::Bold));
            if !detail.is_empty() {
                ui.label(w::text(detail, 15.0, th.muted, Weight::Medium));
            }
            if let (None, Some(step)) = (now, &l.step) {
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if f::inline_button(ui, th, &t("Start focusing"), Kind::Filled) {
                        if let Err(e) = s.focus.set_focus_task(step.id) {
                            self.notice.fail(t(&e.to_string()));
                        } else if !s.focus.state().running {
                            s.focus.toggle();
                        }
                    }
                    if f::inline_button(ui, th, &t("Done, what's next?"), Kind::Text) {
                        if let Some(id) = step.id {
                            if let Err(e) = s.tasks.set_done(id, true) {
                                self.notice.fail(t(&e.to_string()));
                            }
                        }
                    }
                });
            }
        });
    }

    fn agenda_card(&self, ui: &mut Ui, th: &Tokens, a: &DayAgenda) {
        w::card(ui, th, |ui| {
            ui.label(w::text(t("Agenda"), 18.0, th.text, Weight::SemiBold));
            ui.add_space(6.0);
            if a.items.is_empty() {
                let key = if a.is_today { "Nothing planned today" } else { "Nothing planned" };
                ui.label(w::text(t(key), 15.0, th.muted, Weight::Regular));
            }
            for item in &a.items {
                let color = f::hex_color(&item.color).unwrap_or(th.accent);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::hover());
                let p = ui.painter_at(rect);
                p.rect_filled(rect, CornerRadius::same(12), color.gamma_multiply(if th.dark { 0.22 } else { 0.14 }));
                p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(5.0, rect.height())), CornerRadius::same(2), color);
                let when = if item.is_all_day() { t("All day") } else { fmt_range(&item.time) };
                let g = w::fit(ui, &when, 14.0, 12.0, Weight::SemiBold, th.muted, 110.0);
                p.galley(Pos2::new(rect.left() + 16.0, rect.center().y - g.size().y / 2.0), g, th.muted);
                let mut title = item.title.clone();
                if !item.room.is_empty() {
                    title = format!("{title} · {}", item.room);
                }
                let g = w::fit(ui, &title, 16.0, 13.0, Weight::Medium, th.text, rect.width() - 140.0);
                p.galley(Pos2::new(rect.left() + 128.0, rect.center().y - g.size().y / 2.0), g, th.text);
                ui.add_space(6.0);
            }
        });
    }

    fn due_card(&mut self, ui: &mut Ui, th: &Tokens, s: &Services, a: &DayAgenda, today: NaiveDate) {
        w::card(ui, th, |ui| {
            ui.label(w::text(t("Due"), 18.0, th.text, Weight::SemiBold));
            if a.tasks.is_empty() {
                ui.label(w::text(t("Nothing due. Nice."), 15.0, th.muted, Weight::Regular));
            }
            for item in &a.tasks {
                ui.horizontal(|ui| {
                    if f::checkbox(ui, th, item.task.done) {
                        if let Some(id) = item.task.id {
                            if let Err(e) = s.tasks.set_done(id, !item.task.done) {
                                self.notice.fail(t(&e.to_string()));
                            }
                        }
                    }
                    ui.vertical(|ui| {
                        let ink = if item.task.done { th.faint } else { th.text };
                        ui.label(w::text(&item.task.title, 16.0, ink, Weight::Medium));
                        let mut parts = vec![];
                        if let Some(c) = &item.course {
                            parts.push(c.name.clone());
                        }
                        if item.overdue {
                            if let Some(d) = item.task.due {
                                parts.push(was_due(d, today));
                            }
                        }
                        if !parts.is_empty() {
                            let ink = if item.overdue { th.danger } else { th.muted };
                            ui.label(w::text(parts.join(" · "), 13.5, ink, Weight::Regular));
                        }
                    });
                });
            }
            ui.add_space(6.0);
            let r = f::field(ui, th, "", &mut self.quick, &t("Add a task for this day"));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.quick.trim().is_empty() {
                let input = TaskInput { due: Some(self.day), ..TaskInput::titled(self.quick.trim()) };
                match s.tasks.save(None, &input) {
                    Ok(_) => self.quick.clear(),
                    Err(e) => self.notice.fail(t(&e.to_string())),
                }
                r.request_focus();
            }
        });
    }

    fn notes_card(&mut self, ui: &mut Ui, th: &Tokens) {
        w::card(ui, th, |ui| {
            ui.label(w::text(t("Day notes"), 18.0, th.text, Weight::SemiBold));
            ui.add_space(4.0);
            if f::area(ui, th, "", &mut self.journal, &t("Anything to remember about this day…"), 4).changed() {
                self.journal_edit = Some(ui.input(|i| i.time));
            }
        });
    }
}
