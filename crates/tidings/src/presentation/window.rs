//! One window: a rail (or a top row when narrow) picks Top stories, Blindspots, Markets or Sources;
//! fetching runs on a worker thread so the window stays live. Material 3, no glass.

use std::sync::mpsc::{channel, Receiver, Sender};

use egui::{Align, Layout, Vec2};
use quezka_kit::fonts::Weight;
use quezka_kit::theme::GAP;
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::Tokens;

use super::i18n::{t, tf};
use crate::application::{ticker_source, News, Report};

/// Pull the feeds again when the newest we have is older than this (also on open).
const REFRESH_EVERY: i64 = 30 * 60;
use crate::domain::{ago, Lean, Side, Story, Ticker};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Stories,
    Blindspots,
    Markets,
    Sources,
}

const TABS: [(Tab, &str, &str); 4] = [
    (Tab::Stories, "list", "Top stories"),
    (Tab::Blindspots, "eye", "Blindspots"),
    (Tab::Markets, "bolt", "Markets"),
    (Tab::Sources, "tag", "Sources"),
];

enum Event {
    Fetched(Vec<crate::domain::Article>, Report),
}

pub struct TidingsWindow {
    news: News,
    tab: Tab,
    language: Option<String>,
    refreshing: bool,
    message: String,
    add_symbol: String,
    add_name: String,
    src_name: String,
    src_feed: String,
    sender: Sender<Event>,
    events: Receiver<Event>,
    first: bool,
    demo: bool,
}

impl TidingsWindow {
    pub fn new(news: News, demo: bool) -> Self {
        let (sender, events) = channel();
        Self {
            news, tab: Tab::Stories, language: None, refreshing: false, message: String::new(),
            add_symbol: String::new(), add_name: String::new(), src_name: String::new(), src_feed: String::new(),
            sender, events, first: true, demo,
        }
    }

    pub fn open(&mut self, tab: &str) {
        self.tab = match tab {
            "blindspots" => Tab::Blindspots,
            "markets" => Tab::Markets,
            "sources" => Tab::Sources,
            _ => Tab::Stories,
        };
    }

    /// Screenshots: load the demo data now so the window isn't empty.
    pub fn settle(&mut self) {
        if self.demo && self.news.articles.is_empty() {
            let (fresh, _) = News::fetch_all(self.news.feeds().as_ref(), &self.news.jobs());
            self.news.merge(fresh);
        }
        self.first = false;
    }

    fn refresh(&mut self) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.message.clear();
        let (feeds, jobs, tx) = (self.news.feeds(), self.news.jobs(), self.sender.clone());
        std::thread::spawn(move || {
            let (fresh, report) = News::fetch_all(feeds.as_ref(), &jobs);
            let _ = tx.send(Event::Fetched(fresh, report));
        });
    }

    fn pump(&mut self, ctx: &egui::Context) {
        if self.first {
            self.first = false;
            self.refresh(); // on open: fill an empty list, or freshen a stale one
        } else if !self.refreshing && self.news.now() - self.news.newest() > REFRESH_EVERY {
            self.refresh(); // every half hour while it's open
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(60));
        while let Ok(Event::Fetched(fresh, report)) = self.events.try_recv() {
            self.news.merge(fresh);
            self.refreshing = false;
            self.message = if report.failed.is_empty() {
                t("Added the latest news.")
            } else {
                tf("Couldn't reach: {names}", &[("names", &report.failed.join(", "))])
            };
        }
        if self.refreshing {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        self.pump(ui.ctx());
        let narrow = ui.available_width() < 720.0;
        ui.horizontal_top(|ui| {
            if !narrow {
                ui.allocate_ui_with_layout(Vec2::new(190.0, ui.available_height()), Layout::top_down(Align::Min), |ui| {
                    ui.add_space(GAP);
                    ui.label(w::text(t("Tidings"), 22.0, th.text, Weight::Bold));
                    ui.add_space(GAP);
                    for (tab, glyph, name) in TABS {
                        if w::sidebar_item(ui, th, glyph, &t(name), self.tab == tab) {
                            self.tab = tab;
                        }
                    }
                });
                ui.separator();
            }
            ui.vertical(|ui| {
                self.header(ui, th, narrow);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    match self.tab {
                        Tab::Stories => self.stories(ui, th, false),
                        Tab::Blindspots => self.stories(ui, th, true),
                        Tab::Markets => self.markets(ui, th),
                        Tab::Sources => self.sources(ui, th),
                    }
                    ui.add_space(40.0);
                });
            });
        });
    }

    fn header(&mut self, ui: &mut egui::Ui, th: &Tokens, narrow: bool) {
        if narrow {
            let names: Vec<String> = TABS.iter().map(|(_, _, n)| t(n)).collect();
            let at = TABS.iter().position(|(x, _, _)| *x == self.tab).unwrap_or(0);
            if let Some(i) = w::segmented(ui, th, &names.iter().map(String::as_str).collect::<Vec<_>>(), at) {
                self.tab = TABS[i].0;
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if matches!(self.tab, Tab::Stories | Tab::Blindspots) {
                let langs = [("", "All languages"), ("en", "English"), ("it", "Italian")];
                let at = langs.iter().position(|(c, _)| self.language.as_deref() == (if c.is_empty() { None } else { Some(*c) })).unwrap_or(0);
                let names: Vec<String> = langs.iter().map(|(_, n)| t(n)).collect();
                ui.allocate_ui_with_layout(Vec2::new(300.0_f32.min(ui.available_width() - 130.0), 48.0), Layout::left_to_right(Align::Center), |ui| {
                    if let Some(i) = w::segmented(ui, th, &names.iter().map(String::as_str).collect::<Vec<_>>(), at) {
                        self.language = (!langs[i].0.is_empty()).then(|| langs[i].0.to_string());
                    }
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if self.refreshing { t("Refreshing…") } else { t("Refresh") };
                ui.allocate_ui_with_layout(Vec2::new(150.0, 44.0), Layout::top_down(Align::Center), |ui| {
                    if w::button(ui, th, &label, Some("refresh"), Kind::Tonal, 42.0) && !self.refreshing {
                        self.refresh();
                    }
                });
            });
        });
        if !self.message.is_empty() {
            w::small(ui, th, &self.message);
        }
    }

    fn stories(&mut self, ui: &mut egui::Ui, th: &Tokens, blindspots_only: bool) {
        let lang = self.language.clone();
        let stories = if blindspots_only { self.news.blindspots(lang.as_deref()) } else { self.news.stories(lang.as_deref()) };
        if stories.is_empty() {
            w::body(ui, th, &if blindspots_only { t("No blindspots right now. Every story here has both sides.") }
                else { t("No stories yet. Press Refresh to pull the latest news.") });
            return;
        }
        let now = self.news.now();
        for story in &stories {
            self.story_card(ui, th, story, now);
            ui.add_space(10.0);
        }
    }

    fn story_card(&self, ui: &mut egui::Ui, th: &Tokens, story: &Story, now: i64) {
        w::card(ui, th, |ui| {
            ui.label(w::text(&story.title, 18.0, th.text, Weight::SemiBold));
            ui.add_space(6.0);
            coverage_bar(ui, th, story);
            ui.add_space(6.0);
            let (u, unit) = ago(now, story.newest);
            w::small(ui, th, &tf("{n} · {u} {unit} ago", &[("n", &tf("{n} sources", &[("n", &story.sources.to_string())])), ("u", &u.to_string()), ("unit", &t(unit))]));
            if let Some(side) = story.blindspot() {
                ui.add_space(4.0);
                let text = if side == Side::Right { t("The right isn't covering this") } else { t("The left isn't covering this") };
                w::status_chip(ui, &text, th.warning, ui.available_width());
            }
        });
    }

    fn markets(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        w::settings_card(ui, th, Some(&t("Follow a stock")), |ui| {
            quezka_kit::forms::field(ui, th, &t("Symbol (AAPL)"), &mut self.add_symbol, "AAPL");
            quezka_kit::forms::field(ui, th, &t("Name (Apple)"), &mut self.add_name, "Apple");
            if w::button(ui, th, &t("Follow"), Some("add"), Kind::Filled, 42.0) {
                match self.news.follow(&self.add_symbol.clone(), &self.add_name.clone()) {
                    Ok(()) => { self.add_symbol.clear(); self.add_name.clear(); self.message.clear(); self.refresh(); }
                    Err(e) => self.message = t(e),
                }
            }
        });
        ui.add_space(GAP);
        let watchlist = self.news.watchlist.clone();
        if watchlist.is_empty() {
            w::body(ui, th, &t("No stocks yet. Add one above to see its news."));
            return;
        }
        let now = self.news.now();
        let mut unfollow = None;
        for ticker in &watchlist {
            self.ticker_card(ui, th, ticker, now, &mut unfollow);
            ui.add_space(10.0);
        }
        if let Some(sym) = unfollow {
            self.news.unfollow(&sym);
        }
    }

    fn ticker_card(&self, ui: &mut egui::Ui, th: &Tokens, ticker: &Ticker, now: i64, unfollow: &mut Option<String>) {
        w::card(ui, th, |ui| {
            ui.horizontal(|ui| {
                let heading = if ticker.name.is_empty() { ticker.symbol.clone() } else { format!("{} · {}", ticker.symbol, ticker.name) };
                ui.label(w::text(heading, 18.0, th.text, Weight::Bold));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.allocate_ui_with_layout(Vec2::new(120.0, 36.0), Layout::top_down(Align::Center), |ui| {
                        if w::button(ui, th, &t("Unfollow"), None, Kind::Text, 34.0) {
                            *unfollow = Some(ticker.symbol.clone());
                        }
                    });
                });
            });
            let news = self.news.market_news(ticker);
            if news.is_empty() {
                w::small(ui, th, &t("No news yet for the stocks you follow. Press Refresh."));
            }
            for article in news.iter().take(6) {
                ui.add_space(6.0);
                ui.label(w::text(&article.title, 15.0, th.text, Weight::Medium));
                if article.published > 0 {
                    let (u, unit) = ago(now, article.published);
                    let from = if article.source == ticker_source(&ticker.symbol) { String::new() } else { String::new() };
                    let _ = from;
                    w::small(ui, th, &tf("{n} · {u} {unit} ago", &[("n", &self.source_name(&article.source)), ("u", &u.to_string()), ("unit", &t(unit))]));
                }
            }
        });
    }

    fn source_name(&self, id: &str) -> String {
        if let Some(sym) = id.strip_prefix("ticker:") {
            return sym.to_string();
        }
        self.news.source(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string())
    }

    fn sources(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        w::body(ui, th, &t("Left ↔ Right, and on/off — where news groups and blindspots come from."));
        ui.add_space(GAP);
        w::settings_card(ui, th, Some(&t("Add a source")), |ui| {
            quezka_kit::forms::field(ui, th, &t("Name"), &mut self.src_name, "");
            quezka_kit::forms::field(ui, th, &t("Feed address (https://…)"), &mut self.src_feed, "https://");
            if w::button(ui, th, &t("Add"), Some("add"), Kind::Filled, 42.0) {
                let lang = self.language.clone().unwrap_or_else(|| "en".into());
                match self.news.add_source(&self.src_name.clone(), &self.src_feed.clone(), &lang) {
                    Ok(()) => { self.src_name.clear(); self.src_feed.clear(); self.message.clear(); }
                    Err(e) => self.message = t(e),
                }
            }
        });
        ui.add_space(GAP);
        let sources = self.news.sources.clone();
        let leans = [Lean::Left, Lean::CenterLeft, Lean::Center, Lean::CenterRight, Lean::Right];
        for src in &sources {
            w::card(ui, th, |ui| {
                ui.horizontal(|ui| {
                    ui.label(w::text(&src.name, 17.0, th.text, Weight::SemiBold));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if w::switch(ui, th, src.on) {
                            self.news.set_on(&src.id, !src.on);
                        }
                    });
                });
                ui.add_space(4.0);
                let at = leans.iter().position(|l| *l == src.lean).unwrap_or(2);
                let labels = [t("Left"), "·".into(), t("Center"), "·".into(), t("Right")];
                // own id per source <- all cards share the same option labels
                let chosen = ui.push_id(&src.id, |ui| w::segmented(ui, th, &labels.iter().map(String::as_str).collect::<Vec<_>>(), at)).inner;
                if let Some(i) = chosen {
                    self.news.set_lean(&src.id, leans[i]);
                }
            });
            ui.add_space(8.0);
        }
    }
}

/// A Left / Center / Right bar, each band sized to how many sources cover it.
fn coverage_bar(ui: &mut egui::Ui, th: &Tokens, story: &Story) {
    let total = (story.left + story.center + story.right).max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 10.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(5), th.hover);
    let mut x = rect.left();
    for (count, colour) in [(story.left, lean_colour(th, Side::Left)), (story.center, lean_colour(th, Side::Center)), (story.right, lean_colour(th, Side::Right))] {
        if count == 0 {
            continue;
        }
        let w = rect.width() * count as f32 / total;
        let band = egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(w, rect.height()));
        painter.rect_filled(band, egui::CornerRadius::same(5), colour);
        x += w;
    }
}

fn lean_colour(th: &Tokens, side: Side) -> egui::Color32 {
    match side {
        Side::Left => egui::Color32::from_rgb(0x3b, 0x82, 0xf6),
        Side::Center => th.muted,
        Side::Right => egui::Color32::from_rgb(0xef, 0x44, 0x44),
    }
}
