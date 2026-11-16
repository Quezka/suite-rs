//! ideas: catch one (type, enter), find again (search, #tags), act (task, done); delete has undo (= python `views/ideas.py`)

use egui::Ui;
use quezka_kit::fonts::Weight;
use quezka_kit::forms as f;
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::Tokens;

use super::common::{page_title, Notice};
use super::i18n::t;
use crate::application::services::Services;
use crate::domain::ideas::Idea;

const MAX_TAG_CHIPS: usize = 8; // more = clutter

pub struct IdeasPage {
    entry: String,
    query: String,
    show_done: bool,
    loaded: Option<(u64, String, bool, Vec<Idea>, Vec<String>)>,
    deleted: Option<Idea>,
    editing: Option<(i64, String)>,
    notice: Notice,
    pub focus_entry: bool,
}

impl Default for IdeasPage {
    fn default() -> Self {
        Self { entry: String::new(), query: String::new(), show_done: false, loaded: None, deleted: None, editing: None,
            notice: Notice::default(), focus_entry: false }
    }
}

impl IdeasPage {
    fn load(&mut self, s: &Services, count: u64) {
        if self.loaded.as_ref().is_some_and(|l| l.0 == count && l.1 == self.query && l.2 == self.show_done) {
            return;
        }
        let ideas = s.ideas.ideas(self.show_done, &self.query).unwrap_or_default();
        let tags = s.ideas.all_tags().unwrap_or_default();
        self.loaded = Some((count, self.query.clone(), self.show_done, ideas, tags));
    }

    fn toggle_tag(&mut self, tag: &str) {
        let word = format!("#{tag}");
        let mut words: Vec<String> = self.query.split_whitespace().map(String::from).collect();
        if words.contains(&word) {
            words.retain(|x| *x != word);
        } else {
            words.push(word);
        }
        self.query = words.join(" ");
    }

    pub fn ui(&mut self, ui: &mut Ui, th: &Tokens, s: &Services, count: u64) {
        self.load(s, count);
        page_title(ui, th, &t("Ideas"), "");
        ui.add_space(8.0);
        let (_, _, _, ideas, tags) = self.loaded.clone().unwrap_or_default();
        w::card(ui, th, |ui| {
            let r = f::field(ui, th, "", &mut self.entry, &t("Jot an idea… (#tag to sort it)"));
            if self.focus_entry {
                r.request_focus();
                self.focus_entry = false;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.entry.trim().is_empty() {
                match s.ideas.add(&self.entry) {
                    Ok(_) => {
                        self.entry.clear();
                        self.notice.say(t("Caught."));
                    }
                    Err(e) => self.notice.fail(t(&e.to_string())),
                }
                r.request_focus();
            }
            ui.label(w::text(t("Catch it now, look at it later: new ideas come back in the evening digest."), 14.0, th.muted, Weight::Regular));
            ui.horizontal(|ui| {
                self.notice.ui(ui, th);
                if self.deleted.is_some() && f::inline_button(ui, th, &t("Undo"), Kind::Text) {
                    if let Some(idea) = self.deleted.take() {
                        let _ = s.ideas.restore(&idea);
                    }
                    self.notice.text.clear();
                }
            });
        });
        w::card(ui, th, |ui| {
            f::field(ui, th, "", &mut self.query, &t("Search ideas"));
            ui.horizontal_wrapped(|ui| {
                let words: Vec<&str> = self.query.split_whitespace().collect();
                let mut flip = None;
                for tag in tags.iter().take(MAX_TAG_CHIPS) {
                    if w::filter_chip(ui, th, &format!("#{tag}"), words.contains(&format!("#{tag}").as_str())) {
                        flip = Some(tag.clone());
                    }
                }
                if let Some(tag) = flip {
                    self.toggle_tag(&tag);
                }
            });
            if w::switch_row(ui, th, &t("Show the ones done"), self.show_done) {
                self.show_done = !self.show_done;
            }
            if ideas.is_empty() {
                let empty = if self.query.trim().is_empty() { "No ideas yet. The next one that comes to mind goes here." }
                    else { "Nothing matches. Try fewer words." };
                ui.label(w::text(t(empty), 15.0, th.muted, Weight::Regular));
            }
            for idea in &ideas {
                let Some(id) = idea.id else { continue };
                ui.horizontal(|ui| {
                    if f::checkbox(ui, th, idea.done) {
                        let _ = s.ideas.set_done(id, !idea.done);
                    }
                    let ink = if idea.done { th.faint } else { th.text };
                    let width = ui.available_width() - 56.0;
                    ui.allocate_ui(egui::Vec2::new(width, 0.0), |ui| {
                        ui.set_width(width);
                        let r = ui.add(egui::Label::new(w::text(&idea.text, 16.0, ink, Weight::Regular)).wrap().sense(egui::Sense::click()));
                        if r.double_clicked() {
                            self.editing = Some((id, idea.text.clone()));
                        }
                    });
                    let more = ui.add(egui::Button::new(w::text("⋯", 20.0, th.muted, Weight::Bold)).frame(false).min_size(egui::Vec2::splat(44.0)));
                    f::menu(th, &more, |ui| {
                        if f::menu_item(ui, th, Some("today"), &t("Make it a task for today"), false) {
                            match s.ideas.to_task(id, Some(s.planner.today())) {
                                Ok(_) => self.notice.say(t("Now a task for today.")),
                                Err(e) => self.notice.fail(t(&e.to_string())),
                            }
                        }
                        if f::menu_item(ui, th, Some("checklist"), &t("Make it a task"), false) {
                            match s.ideas.to_task(id, None) {
                                Ok(_) => self.notice.say(t("Now a task.")),
                                Err(e) => self.notice.fail(t(&e.to_string())),
                            }
                        }
                        if f::menu_item(ui, th, Some("edit"), &t("Edit"), false) {
                            self.editing = Some((id, idea.text.clone()));
                        }
                        if f::menu_item(ui, th, Some("trash"), &t("Delete"), true) {
                            match s.ideas.delete(id) {
                                Ok(()) => {
                                    self.deleted = Some(idea.clone());
                                    self.notice.say(t("Idea deleted."));
                                }
                                Err(e) => self.notice.fail(t(&e.to_string())),
                            }
                        }
                    });
                });
            }
        });
        self.edit_dialog(ui, th, s);
    }

    fn edit_dialog(&mut self, ui: &mut Ui, th: &Tokens, s: &Services) {
        let Some((id, mut text)) = self.editing.take() else { return };
        let mut close = false;
        let out = f::dialog(ui.ctx(), th, ui.scope_id().with("edit-idea"), &t("Edit idea"), |ui| {
            f::area(ui, th, "", &mut text, "", 4);
            match f::dialog_buttons(ui, th, &[(&t("Cancel"), Kind::Text), (&t("Save"), Kind::Filled)]) {
                Some(1) if !text.trim().is_empty() => {
                    if let Err(e) = s.ideas.edit(id, &text) {
                        self.notice.fail(t(&e.to_string()));
                    }
                    close = true;
                }
                Some(_) => close = true,
                None => {}
            }
        });
        if !close && !out.dismissed {
            self.editing = Some((id, text));
        }
    }
}
