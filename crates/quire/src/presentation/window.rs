//! quire window: sidebar (expanded) | icon rail (medium) | top tabs (compact) + page beside it

use egui::Ui;
use quezka_kit::fonts::Weight;
use quezka_kit::forms::{width_class, WidthClass};
use quezka_kit::theme::GAP;
use quezka_kit::widgets as w;
use quezka_kit::Tokens;

use super::common::Changes;
use super::i18n::t;
use super::ideas::IdeasPage;
use super::today::TodayPage;
use crate::application::services::Services;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Today,
    Ideas,
}

const PAGES: [(Page, &str, &str); 2] = [(Page::Today, "today", "Today"), (Page::Ideas, "idea", "Ideas")];

pub struct QuireWindow {
    pub services: Services,
    changes: Changes,
    pub page: Page,
    today: TodayPage,
    ideas: IdeasPage,
}

impl QuireWindow {
    pub fn new(services: Services) -> Self {
        let changes = Changes::watch(&services.bus);
        let today = TodayPage::new(services.planner.today());
        Self { services, changes, page: Page::Today, today, ideas: IdeasPage::default() }
    }

    fn keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::I)) {
            self.page = Page::Ideas;
            self.ideas.focus_entry = true;
        }
        for (i, key) in [Key::Num1, Key::Num2].into_iter().enumerate() {
            if ctx.input_mut(|inp| inp.consume_key(Modifiers::COMMAND, key)) {
                self.page = PAGES[i].0;
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, th: &Tokens) {
        self.keys(ui.ctx());
        let width = width_class(ui.available_width());
        egui::Frame::new().fill(th.bg).inner_margin(egui::Margin::same(GAP as i8)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match width {
                WidthClass::Compact => {
                    ui.horizontal(|ui| {
                        for (page, glyph, name) in PAGES {
                            if w::filter_chip(ui, th, &t(name), self.page == page) {
                                let _ = glyph;
                                self.page = page;
                            }
                        }
                    });
                    ui.add_space(GAP);
                    self.page_ui(ui, th, width);
                }
                _ => {
                    let side = if width == WidthClass::Expanded { 230.0 } else { 72.0 };
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui(egui::Vec2::new(side, ui.available_height()), |ui| {
                            ui.set_width(side);
                            ui.vertical(|ui| self.sidebar(ui, th, width == WidthClass::Expanded));
                        });
                        ui.add_space(GAP);
                        ui.vertical(|ui| self.page_ui(ui, th, width));
                    });
                }
            }
        });
    }

    fn sidebar(&mut self, ui: &mut Ui, th: &Tokens, labels: bool) {
        if labels {
            ui.label(w::text("Quire", 26.0, th.text, Weight::Bold));
            ui.add_space(GAP);
        }
        for (page, glyph, name) in PAGES {
            let chosen = self.page == page;
            let tapped = if labels {
                w::sidebar_item(ui, th, glyph, &t(name), chosen)
            } else {
                w::nav_item(ui, th, glyph, &t(name), chosen)
            };
            if tapped {
                if self.page == Page::Today {
                    self.today.flush_journal(&self.services);
                }
                self.page = page;
            }
        }
    }

    fn page_ui(&mut self, ui: &mut Ui, th: &Tokens, width: WidthClass) {
        let count = self.changes.count();
        match self.page {
            Page::Today => self.today.ui(ui, th, &self.services, count, width),
            Page::Ideas => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.ideas.ui(ui, th, &self.services, count));
            }
        }
    }

    /// before quit: unsaved day notes
    pub fn flush(&mut self) {
        self.today.flush_journal(&self.services);
    }
}
