//! One window: a card per app of the suite with Install, Update or Open, under a glass header.

use std::sync::mpsc::{channel, Receiver, Sender};

use egui::{Align, Align2, Layout, Order, Pos2, Rect, Vec2};
use quezka_kit::fonts::Weight;
use quezka_kit::glass::{self, Weight as Glass};
use quezka_kit::theme::{GAP, OUTER_RADIUS};
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::{App, Tokens};

use super::i18n::{t, tf};
use crate::application::service::{download_and_install, AppStatus, Releases, State, SuiteService};
use crate::application::QuayError;

const ICON: f32 = 52.0;
const HEADER: f32 = 76.0;

/// What a background job tells the window.
enum Event {
    Checked(Result<Releases, QuayError>),
    Progress(&'static str, u64, u64),
    Installing(&'static str),
    Installed(&'static str, Result<(), QuayError>),
}

pub struct QuayWindow {
    service: SuiteService,
    icon: fn(&str) -> Option<&'static [u8]>,
    statuses: Vec<AppStatus>,
    message: String,
    checking: bool,
    /// The app being downloaded or installed, with its progress (None while the installer runs).
    busy: Option<(&'static str, Option<f32>)>,
    queue: Vec<&'static str>,
    failed: Vec<String>,
    queue_total: usize,
    sender: Sender<Event>,
    events: Receiver<Event>,
    ctx: Option<egui::Context>,
    /// Started at login: stay out of the way unless an update is waiting.
    pub quiet: bool,
    pub wants_close: bool,
    pub wants_show: bool,
    scroll: f32,
}

/// A kit button `width` points wide (they fill whatever width they're given).
fn sized(ui: &mut egui::Ui, th: &Tokens, width: f32, label: &str, glyph: Option<&'static str>, kind: Kind) -> bool {
    ui.allocate_ui(Vec2::new(width, 46.0), |ui| {
        ui.set_width(width);
        w::button(ui, th, label, glyph, kind, 44.0)
    })
    .inner
}

fn rank(s: &AppStatus) -> u8 {
    // What needs you comes first: updates, then what's installed, then the rest.
    match (s.state, &s.installed) {
        (State::Update, _) => 0,
        (_, Some(_)) => 1,
        _ => 2,
    }
}

impl QuayWindow {
    pub fn new(service: SuiteService, icon: fn(&str) -> Option<&'static [u8]>, quiet: bool) -> Self {
        let (sender, events) = channel();
        let mut window = Self {
            statuses: service.statuses(),
            service,
            icon,
            message: String::new(),
            checking: false,
            busy: None,
            queue: Vec::new(),
            failed: Vec::new(),
            queue_total: 0,
            sender,
            events,
            ctx: None,
            quiet,
            wants_close: false,
            wants_show: false,
            scroll: 0.0,
        };
        window.check();
        window
    }

    /// For screenshots and tests: wait for the background check to finish.
    pub fn settle(&mut self) {
        while self.checking || self.busy.is_some() {
            match self.events.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok(e) => self.handle(e),
                Err(_) => break,
            }
        }
    }

    pub fn check(&mut self) {
        self.checking = true;
        self.message = t("Checking…");
        let feed = self.service.feed();
        let apps = self.service.apps().to_vec();
        let tx = self.sender.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Checked(SuiteService::fetch(&*feed, &apps)));
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }

    fn install(&mut self, key: &'static str) {
        let asset = match self.service.asset_for(key) {
            Ok(a) => a,
            Err(e) => {
                self.finished(key, Err(e));
                return;
            }
        };
        self.busy = Some((key, Some(0.0)));
        let (feed, platform, tx, ctx) = (self.service.feed(), self.service.platform(), self.sender.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let poke = || {
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            };
            let progress = |done, total| {
                let _ = tx.send(Event::Progress(key, done, total));
                poke();
            };
            let installing = || {
                let _ = tx.send(Event::Installing(key));
                poke();
            };
            let result = download_and_install(&*feed, &*platform, key, &asset, &progress, &installing);
            let _ = tx.send(Event::Installed(key, result));
            poke();
        });
    }

    fn install_all(&mut self) {
        self.queue = self.service.pending_updates();
        self.queue_total = self.queue.len();
        self.failed.clear();
        self.next_in_queue();
    }

    fn next_in_queue(&mut self) {
        if self.queue.is_empty() {
            if self.queue_total > 0 {
                self.message = if self.failed.is_empty() {
                    t("All up to date.")
                } else {
                    tf("Not updated: {names}.", &[("names", &self.failed.join(", "))])
                };
                self.queue_total = 0;
            }
            return;
        }
        let key = self.queue.remove(0);
        let done = (self.queue_total - self.queue.len()).to_string();
        self.message = tf("Updating {done} of {total}…", &[("done", &done), ("total", &self.queue_total.to_string())]);
        self.install(key);
    }

    fn name(key: &str) -> &'static str {
        crate::domain::catalog::by_key(key).map(|a| a.name).unwrap_or("")
    }

    fn finished(&mut self, key: &'static str, result: Result<(), QuayError>) {
        self.busy = None;
        match result {
            Ok(()) => {
                if self.queue_total == 0 {
                    self.message = tf("{name} is installed.", &[("name", Self::name(key))]);
                }
            }
            Err(e) => {
                self.message = tf("{name}: {error}", &[("name", Self::name(key)), ("error", &t(&e.0))]);
                self.failed.push(Self::name(key).to_string());
            }
        }
        self.statuses = self.service.statuses();
        self.next_in_queue();
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Checked(Ok(releases)) => {
                self.checking = false;
                self.message.clear();
                self.service.remember(releases);
                self.statuses = self.service.statuses();
                if self.quiet {
                    self.quiet = false;
                    if self.service.has_updates() { self.wants_show = true } else { self.wants_close = true }
                }
            }
            Event::Checked(Err(e)) => {
                self.checking = false;
                self.message = t(&e.0);
                if self.quiet {
                    self.wants_close = true; // offline at login: nothing to say
                }
            }
            Event::Progress(key, done, total) => self.busy = Some((key, Some(done as f32 / total.max(1) as f32))),
            Event::Installing(key) => {
                self.busy = Some((key, None));
                self.message = t("Installing…");
            }
            Event::Installed(key, result) => self.finished(key, result),
        }
    }

    fn detail(s: &AppStatus) -> String {
        let latest = s.latest.as_deref().unwrap_or("");
        let installed = s.installed.as_deref().unwrap_or("");
        match s.state {
            State::Unavailable => t("No download for this computer yet"),
            State::NotInstalled => tf("Not installed · latest {latest}", &[("latest", latest)]),
            State::Update => tf("Installed {installed} · {latest} is available", &[("installed", installed), ("latest", latest)]),
            State::UpToDate => tf("Installed {installed} · up to date", &[("installed", installed)]),
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, th: &Tokens, s: &AppStatus) {
        w::card(ui, th, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON), egui::Sense::hover());
                let drawn = (self.icon)(s.app.key).is_some_and(|svg| quezka_kit::icons::picture(ui.painter(), rect, s.app.key, svg));
                if !drawn {
                    ui.painter().rect_filled(rect, 14.0, th.accent_soft);
                }
                ui.add_space(12.0);
                let buttons = 330.0_f32.min(ui.available_width() * 0.55);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - buttons).max(120.0));
                    ui.label(w::text(s.app.name, 19.0, th.text, Weight::Bold));
                    ui.label(w::text(t(s.app.tagline), 15.0, th.muted, Weight::Regular));
                    ui.label(w::text(Self::detail(s), 14.0, th.muted, Weight::Medium));
                    if s.state == State::Update && !s.page_url.is_empty() {
                        let link = ui.add(egui::Label::new(w::text(t("What's new"), 14.0, th.accent, Weight::Bold)).sense(egui::Sense::click()));
                        if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&s.page_url));
                        }
                    }
                    if let Some((key, fraction)) = self.busy {
                        if key == s.app.key {
                            ui.add_space(6.0);
                            match fraction {
                                Some(f) => w::progress(ui, th, egui::Id::NULL.with(("quay-progress", key)), f, th.accent, 8.0),
                                None => w::indeterminate(ui, th),
                            }
                        }
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let idle = self.busy.is_none();
                    if s.installed.is_some() && sized(ui, th, 150.0, &t("Open"), Some("play"), Kind::Outlined) {
                        if let Err(e) = self.service.launch(s.app.key) {
                            self.message = tf("{name}: {error}", &[("name", s.app.name), ("error", &t(&e.0))]);
                        }
                    }
                    match s.state {
                        State::Update => {
                            if idle && sized(ui, th, 150.0, &t("Update"), Some("refresh"), Kind::Filled) {
                                self.install(s.app.key);
                            }
                        }
                        State::NotInstalled => {
                            if idle && sized(ui, th, 150.0, &t("Install"), Some("download"), Kind::Tonal) {
                                self.install(s.app.key);
                            }
                        }
                        State::Unavailable if !s.page_url.is_empty() => {
                            if sized(ui, th, 150.0, &t("Releases page"), None, Kind::Text) {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(&s.page_url));
                            }
                        }
                        _ => {}
                    }
                });
            });
        });
    }
}

impl App for QuayWindow {
    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        let ctx = root.ctx().clone();
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
        }
        while let Ok(event) = self.events.try_recv() {
            self.handle(event);
        }
        if self.wants_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let screen = ctx.content_rect();
        let header = Rect::from_min_size(screen.min + Vec2::splat(GAP), Vec2::new(screen.width() - 2.0 * GAP, HEADER));
        let mut statuses = self.statuses.clone();
        statuses.sort_by_key(rank);
        let waiting = statuses.iter().filter(|s| s.state == State::Update).count();

        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg).inner_margin(egui::Margin::symmetric(GAP as i8, 0))).show(root, |ui| {
            let out = egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(HEADER + 2.0 * GAP);
                if !self.message.is_empty() {
                    ui.label(w::text(&self.message, 16.0, th.muted, Weight::Medium));
                    ui.add_space(GAP);
                }
                for s in &statuses {
                    self.card(ui, th, s);
                }
                if self.service.autostart_supported() {
                    ui.add_space(GAP);
                    let on = self.service.autostart_enabled();
                    if w::switch_row(ui, th, &t("Start with the computer"), on) {
                        if let Err(e) = self.service.set_autostart(!on) {
                            self.message = t(&e.0);
                        }
                    }
                }
                ui.add_space(GAP);
            });
            self.scroll = out.state.clamped_offset().y;
        });

        egui::Area::new(egui::Id::NULL.with("quay-header")).order(Order::Foreground).fade_in(false).fixed_pos(Pos2::ZERO).show(&ctx, |ui| {
            if self.scroll > 0.0 {
                glass::pane(ui.painter(), header, OUTER_RADIUS, th, Glass::Regular, true);
            }
            let subtitle = if waiting > 0 { tf("Updates waiting: {n}", &[("n", &waiting.to_string())]) } else { t("Install and update the apps") };
            ui.painter().text(header.left_top() + Vec2::new(18.0, 12.0), Align2::LEFT_TOP, t("The suite"), Weight::Bold.font(26.0), th.text);
            ui.painter().text(header.left_top() + Vec2::new(18.0, 46.0), Align2::LEFT_TOP, subtitle, Weight::Regular.font(15.0), th.muted);
            let buttons = Rect::from_min_max(Pos2::new(header.min.x + 260.0, header.min.y + 14.0), header.max - Vec2::new(14.0, 14.0));
            let mut row = ui.new_child(egui::UiBuilder::new().max_rect(buttons).layout(Layout::right_to_left(Align::Center)));
            let all = if waiting > 0 { tf("Update all ({n})", &[("n", &waiting.to_string())]) } else { t("Update all") };
            if waiting > 0 && self.busy.is_none() && sized(&mut row, th, 230.0, &all, Some("download"), Kind::Filled) {
                self.install_all();
            }
            if !self.checking && self.busy.is_none() && sized(&mut row, th, 230.0, &t("Check for updates"), Some("refresh"), Kind::Tonal) {
                self.check();
            }
        });
    }
}

