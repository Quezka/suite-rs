//! One window, laid out like a desktop app: a sidebar (phone status, sections, start-at-login, quit) and the section
//! beside it; Messages = conversation list | thread with bubbles. Narrow window -> tabs on top, one pane.
//! close button hides the window; the link keeps running (kit `close_hides`); Quit ends it

use std::sync::mpsc::Receiver;
use std::time::Duration;

use egui::{Align, CornerRadius, Key, Layout, Modifiers, Vec2};
use quezka_kit::fonts::Weight;
use quezka_kit::theme::GAP;
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::{App, Tokens};

use super::i18n::{t, tf};
use crate::application::{to_phone, Alert, Call, Desktop, LinkEvent, Phone, PhoneLink, Thread};

const SIDEBAR: f32 = 250.0;
const LIST: f32 = 280.0;
const COMPACT_BELOW: f32 = 760.0; // window narrower -> tabs, one pane

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Notifications,
    Messages,
    Files,
    Phone,
}

const SECTIONS: [(Section, &str, &str); 4] = [
    (Section::Notifications, "bell", "Notifications"),
    (Section::Messages, "mail", "Messages"),
    (Section::Files, "folder", "Files"),
    (Section::Phone, "settings", "Phone"),
];

pub struct TetherWindow {
    link: Box<dyn PhoneLink>,
    desktop: Box<dyn Desktop>,
    events: Receiver<LinkEvent>,
    /// Another `tether` was started: bring this window up.
    show_requests: Option<Receiver<()>>,
    phone: Phone,
    section: Section,
    code: Option<String>,
    replies: std::collections::HashMap<String, String>,
    thread: Option<String>,
    draft: String,
    message: String,
}

/// A kit button `width` points wide (kit buttons fill whatever width they get).
fn sized(ui: &mut egui::Ui, th: &Tokens, width: f32, label: &str, glyph: Option<&'static str>, kind: Kind) -> bool {
    ui.allocate_ui(Vec2::new(width, 46.0), |ui| {
        ui.set_width(width);
        w::button(ui, th, label, glyph, kind, 42.0)
    })
    .inner
}

fn who(thread: &Thread) -> String {
    if thread.name.is_empty() { thread.address.clone() } else { thread.name.clone() }
}

impl TetherWindow {
    pub fn new(link: Box<dyn PhoneLink>, desktop: Box<dyn Desktop>, events: Receiver<LinkEvent>) -> Self {
        Self { link, desktop, events, show_requests: None, phone: Phone::default(), section: Section::Notifications, code: None,
            replies: Default::default(), thread: None, draft: String::new(), message: String::new() }
    }

    /// Open on a section (screenshots): 0 notifications, 1 messages, 2 files, 3 phone; and a conversation.
    pub fn open(mut self, section: usize, thread: Option<String>) -> Self {
        self.section = SECTIONS[section.min(3)].0;
        self.thread = thread;
        self
    }

    /// Show the window when this receives (single instance: a second launch asks the first to show).
    pub fn with_show_requests(mut self, requests: Receiver<()>) -> Self {
        self.show_requests = Some(requests);
        self
    }

    fn pump(&mut self, ctx: &egui::Context) {
        if let Some(requests) = &self.show_requests {
            if requests.try_recv().is_ok() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        while let Ok(e) = self.events.try_recv() {
            match e {
                LinkEvent::Connected(name) => { self.phone.connected = true; self.phone.name = name; self.code = None;
                    self.link.send(to_phone::sms_get()); }
                LinkEvent::Disconnected => { self.phone.connected = false; self.phone.call = None; }
                LinkEvent::Paired(name) => { self.code = None; self.message = tf("Connected to {name}", &[("name", &name)]); }
                LinkEvent::FileSaved(path) => {
                    let shown = path.display().to_string();
                    self.desktop.notify("Tether", &tf("Saved to {path}", &[("path", &shown)]));
                    self.message = tf("Saved to {path}", &[("path", &shown)]);
                }
                LinkEvent::Message(m) => match self.phone.apply(&m) {
                    Some(Alert::Notification(n)) => self.desktop.notify(&format!("{} · {}", n.app, n.title), &n.text),
                    Some(Alert::Sms { name, body }) => self.desktop.notify(&name, &body),
                    Some(Alert::Ringing { name }) => self.desktop.notify(&tf("{name} is calling", &[("name", &name)]), ""),
                    Some(Alert::Clipboard(text)) => ctx.copy_text(text),
                    _ => {}
                },
            }
        }
    }

    fn send(&mut self, m: serde_json::Value) {
        if !self.link.send(m) {
            self.message = t("The phone isn't connected.");
        }
    }

    /// Files dropped on the window go to the phone; Ctrl+1..4 pick a section.
    fn input(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        for path in dropped {
            match self.link.send_file(&path) {
                Ok(()) => self.message = tf("Sent {name} to the phone", &[("name", &path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())]),
                Err(e) => self.message = t(&e),
            }
        }
        for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
            if ctx.input_mut(|inp| inp.consume_key(Modifiers::COMMAND, key)) {
                self.section = SECTIONS[i].0;
            }
        }
    }

    pub fn status_line(&self) -> String {
        match (self.phone.connected, self.phone.battery) {
            (true, Some((level, charging))) => format!("{} · {}{}", t("Connected"), tf("Battery {n}%", &[("n", &level.to_string())]),
                if charging { format!(" · {}", t("charging")) } else { String::new() }),
            (true, None) => t("Connected"),
            _ => t("Waiting for the phone…"),
        }
    }

    fn count(&self, section: Section) -> usize {
        match section {
            Section::Notifications => self.phone.notifications.len(),
            Section::Messages => self.phone.threads.len(),
            _ => 0,
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        ui.add_space(GAP);
        ui.label(w::text("Tether", 26.0, th.text, Weight::Bold));
        ui.add_space(GAP);
        // phone card: name + connection dot + battery
        egui::Frame::new().fill(th.surface).corner_radius(CornerRadius::same(16)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, if self.phone.connected { th.success } else { th.faint });
                let name = if self.phone.name.is_empty() { t("Your phone") } else { self.phone.name.clone() };
                ui.label(w::text(name, 17.0, th.text, Weight::Bold));
            });
            ui.label(w::text(self.status_line(), 14.0, th.muted, Weight::Medium));
        });
        ui.add_space(GAP);
        for (section, glyph, name) in SECTIONS {
            let n = self.count(section);
            let label = if n > 0 { format!("{}  ({n})", t(name)) } else { t(name) };
            if w::sidebar_item(ui, th, glyph, &label, self.section == section) {
                self.section = section;
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(GAP);
            if w::button(ui, th, &t("Quit Tether"), Some("x"), Kind::Text, 40.0) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            let on = self.desktop.autostart_enabled();
            if w::switch_row(ui, th, &t("Start with the computer"), on) {
                if let Err(e) = self.desktop.set_autostart(!on) {
                    self.message = t(&e);
                }
            }
            w::small(ui, th, &t("Closing the window keeps Tether running."));
        });
    }

    fn section_ui(&mut self, ui: &mut egui::Ui, th: &Tokens, compact: bool) {
        if !self.message.is_empty() {
            ui.horizontal(|ui| {
                w::small(ui, th, &self.message);
                if w::button(ui, th, &t("OK"), None, Kind::Text, 32.0) {
                    self.message.clear();
                }
            });
        }
        self.call_banner(ui, th);
        match self.section {
            Section::Notifications => { egui::ScrollArea::vertical().show(ui, |ui| self.notifications(ui, th)); }
            Section::Messages => self.messages(ui, th, compact),
            Section::Files => { egui::ScrollArea::vertical().show(ui, |ui| self.files(ui, th)); }
            Section::Phone => { egui::ScrollArea::vertical().show(ui, |ui| self.phone_settings(ui, th)); }
        }
    }
}

impl App for TetherWindow {
    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        let ctx = root.ctx().clone();
        self.pump(&ctx);
        self.input(&ctx);
        ctx.request_repaint_after(Duration::from_millis(300));
        let compact = ctx.content_rect().width() < COMPACT_BELOW;
        if compact {
            egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg).inner_margin(egui::Margin::same(GAP as i8 * 2)))
                .show(root, |ui| {
                    ui.label(w::text("Tether", 26.0, th.text, Weight::Bold));
                    ui.label(w::text(self.status_line(), 15.0, th.muted, Weight::Regular));
                    ui.add_space(GAP);
                    let names: Vec<String> = SECTIONS.iter().map(|(_, _, n)| t(n)).collect();
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    let at = SECTIONS.iter().position(|(s, _, _)| *s == self.section).unwrap_or(0);
                    if let Some(i) = w::segmented(ui, th, &names, at) {
                        self.section = SECTIONS[i].0;
                    }
                    ui.add_space(GAP);
                    self.section_ui(ui, th, true);
                });
            return;
        }
        egui::Panel::left("tether-sidebar").exact_size(SIDEBAR).resizable(false)
            .frame(egui::Frame::new().fill(th.sidebar).inner_margin(egui::Margin::same(GAP as i8 + 4)))
            .show(root, |ui| self.sidebar(ui, th));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg).inner_margin(egui::Margin::same(GAP as i8 * 2)))
            .show(root, |ui| {
                let name = SECTIONS.iter().find(|(s, _, _)| *s == self.section).map(|(_, _, n)| t(n)).unwrap_or_default();
                ui.label(w::text(name, 24.0, th.text, Weight::Bold));
                ui.add_space(GAP);
                self.section_ui(ui, th, false);
            });
    }
}

impl TetherWindow {
    fn call_banner(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        let call = self.phone.call.clone();
        match call {
            Some(Call::Ringing { number, name }) => {
                let who = if name.is_empty() { number } else { name };
                w::card(ui, th, |ui| {
                    ui.label(w::text(tf("{name} is calling", &[("name", &who)]), 20.0, th.text, Weight::Bold));
                    ui.horizontal(|ui| {
                        if w::button(ui, th, &t("Answer"), Some("play"), Kind::Filled, 44.0) { self.link.send(to_phone::answer()); }
                        if w::button(ui, th, &t("Reject"), None, Kind::Tonal, 44.0) { self.link.send(to_phone::reject()); }
                    });
                });
            }
            Some(Call::Talking { number, name }) => {
                let who = if name.is_empty() { number } else { name };
                w::card(ui, th, |ui| {
                    ui.label(w::text(tf("On a call with {name}", &[("name", &who)]), 18.0, th.text, Weight::Bold));
                    if w::button(ui, th, &t("Hang up"), None, Kind::Tonal, 44.0) { self.link.send(to_phone::reject()); }
                });
            }
            _ => {}
        }
    }

    fn phone_settings(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        w::settings_card(ui, th, Some(&t("Phone")), |ui| {
            if self.phone.connected {
                ui.label(w::text(tf("Connected to {name}", &[("name", &self.phone.name)]), 18.0, th.text, Weight::Bold));
                if let Some((level, _)) = self.phone.battery {
                    w::body(ui, th, &tf("Battery {n}%", &[("n", &level.to_string())]));
                }
            } else {
                w::body(ui, th, &t("No phone connected. Open Tether on the phone, on the same Wi-Fi."));
            }
        });
        ui.add_space(GAP);
        let paired = self.link.paired();
        let mut forget = None;
        let mut new_code = false;
        w::settings_card(ui, th, Some(&t("Pair a phone")), |ui| {
            if let Some(code) = &self.code {
                w::small(ui, th, &t("On the phone, pick this computer and type this code:"));
                ui.label(w::text(code, 34.0, th.accent, Weight::Bold));
            } else if w::button(ui, th, &t("Pair a phone"), Some("add"), Kind::Filled, 44.0) {
                new_code = true;
            }
            if !paired.is_empty() {
                ui.add_space(GAP);
                ui.label(w::text(t("Paired phones"), 16.0, th.text, Weight::Bold));
                for (id, name) in &paired {
                    ui.horizontal(|ui| {
                        ui.label(w::text(name, 16.0, th.text, Weight::Medium));
                        if w::button(ui, th, &t("Forget"), None, Kind::Tonal, 36.0) { forget = Some(id.clone()); }
                    });
                }
            }
        });
        if new_code { self.code = Some(self.link.new_code()); }
        if let Some(id) = forget { self.link.forget(&id); }
    }

    fn notifications(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        if self.phone.notifications.is_empty() {
            w::body(ui, th, &t("Nothing new on the phone."));
        }
        let mut actions: Vec<serde_json::Value> = Vec::new();
        for n in self.phone.notifications.clone() {
            w::card(ui, th, |ui| {
                ui.label(w::text(&n.app, 14.0, th.muted, Weight::Bold));
                ui.label(w::text(&n.title, 18.0, th.text, Weight::Bold));
                if !n.text.is_empty() { w::body(ui, th, &n.text); }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if n.can_reply {
                        let draft = self.replies.entry(n.key.clone()).or_default();
                        let width = (ui.available_width() - 2.0 * 140.0 - 16.0).max(160.0);
                        let edit = ui.add(egui::TextEdit::singleline(draft).hint_text(t("Reply…")).desired_width(width)
                            .min_size(Vec2::new(0.0, 40.0)));
                        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if (sized(ui, th, 140.0, &t("Send"), Some("send"), Kind::Filled) || enter) && !draft.trim().is_empty() {
                            actions.push(to_phone::reply(&n.key, draft.trim()));
                            draft.clear();
                        }
                    }
                    if sized(ui, th, 140.0, &t("Dismiss"), None, Kind::Tonal) {
                        actions.push(to_phone::dismiss(&n.key));
                    }
                });
            });
        }
        for a in actions {
            if a["t"] == "dismiss" {
                let key = a["key"].as_str().unwrap_or_default().to_string();
                self.phone.notifications.retain(|n| n.key != key);
            }
            self.send(a);
        }
    }

    fn conversation_list(&mut self, ui: &mut egui::Ui, th: &Tokens, threads: &[Thread]) {
        if w::button(ui, th, &t("Load messages"), Some("refresh"), Kind::Tonal, 40.0) {
            self.send(to_phone::sms_get());
        }
        ui.add_space(GAP / 2.0);
        egui::ScrollArea::vertical().id_salt("threads").show(ui, |ui| {
            for thread in threads {
                let chosen = self.thread.as_deref() == Some(thread.address.as_str());
                let last = thread.messages.last().map(|m| m.body.replace('\n', " ")).unwrap_or_default();
                let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 62.0), egui::Sense::click());
                let fill = if chosen { th.accent_soft } else if response.hovered() { th.hover } else { egui::Color32::TRANSPARENT };
                ui.painter().rect_filled(rect, 14.0, fill);
                let inner = rect.shrink2(Vec2::new(12.0, 8.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                child.label(w::text(who(thread), 16.0, th.text, Weight::Bold));
                let preview = w::fit(&child, &last, 14.0, 14.0, Weight::Regular, th.muted, inner.width());
                child.label(preview);
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.thread = Some(thread.address.clone());
                }
            }
        });
    }

    fn conversation(&mut self, ui: &mut egui::Ui, th: &Tokens, threads: &[Thread]) {
        let Some(address) = self.thread.clone() else {
            ui.centered_and_justified(|ui| { w::body(ui, th, &t("Pick a conversation.")); });
            return;
        };
        let thread = threads.iter().find(|x| x.address == address);
        if let Some(thread) = thread {
            ui.label(w::text(who(thread), 20.0, th.text, Weight::Bold));
        }
        let composer = 110.0;
        let height = (ui.available_height() - composer).max(120.0);
        egui::ScrollArea::vertical().id_salt("thread").max_height(height).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            let Some(thread) = thread else { return };
            let max = (ui.available_width() * 0.7).max(200.0);
            for m in thread.messages.iter().rev().take(80).rev() {
                let layout = if m.incoming { Layout::left_to_right(Align::Min) } else { Layout::right_to_left(Align::Min) };
                ui.with_layout(layout, |ui| {
                    let (fill, ink) = if m.incoming { (th.surface, th.text) } else { (th.accent, th.on_accent) };
                    egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(16)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                        ui.set_max_width(max);
                        ui.add(egui::Label::new(w::text(&m.body, 16.0, ink, Weight::Regular)).wrap());
                    });
                });
                ui.add_space(6.0);
            }
        });
        ui.add_space(GAP / 2.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 130.0).max(160.0);
            let edit = ui.add(egui::TextEdit::multiline(&mut self.draft).hint_text(t("Write a message… (Enter sends)"))
                .desired_rows(2).desired_width(width));
            // Enter sends, Shift+Enter is a new line
            let enter = edit.has_focus() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            if (w::button(ui, th, &t("Send"), Some("send"), Kind::Filled, 44.0) || enter) && !self.draft.trim().is_empty() {
                let body = self.draft.trim().to_string();
                self.draft.clear();
                self.phone.add_sms(&address, "", crate::application::Sms { incoming: false, body: body.clone(), at: i64::MAX });
                self.send(to_phone::sms_send(&address, &body));
            }
        });
    }

    fn messages(&mut self, ui: &mut egui::Ui, th: &Tokens, compact: bool) {
        let threads = self.phone.threads.clone();
        if compact {
            if self.thread.is_some() {
                if w::button(ui, th, &t("All conversations"), Some("chevron-left"), Kind::Text, 40.0) {
                    self.thread = None;
                    return;
                }
                self.conversation(ui, th, &threads);
            } else {
                self.conversation_list(ui, th, &threads);
            }
            return;
        }
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(LIST, height), Layout::top_down(Align::Min), |ui| {
                ui.set_width(LIST);
                self.conversation_list(ui, th, &threads);
            });
            ui.add_space(GAP);
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), height), Layout::top_down(Align::Min), |ui| {
                self.conversation(ui, th, &threads);
            });
        });
    }

    fn files(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        w::settings_card(ui, th, Some(&t("Files")), |ui| {
            if w::button(ui, th, &t("Send a file to the phone"), Some("upload"), Kind::Filled, 44.0) {
                if let Some(path) = self.desktop.pick_file() {
                    if let Err(e) = self.link.send_file(&path) { self.message = t(&e); }
                }
            }
            w::small(ui, th, &t("Or drop files anywhere on this window."));
            w::small(ui, th, &t("Files the phone shares with Tether land in your Downloads folder."));
        });
        ui.add_space(GAP);
        w::settings_card(ui, th, Some(&t("Clipboard")), |ui| {
            if w::button(ui, th, &t("Send my clipboard"), Some("copy"), Kind::Tonal, 44.0) {
                if let Some(text) = self.desktop.clipboard_text() { self.send(to_phone::clip(&text)); }
            }
            if let Some(text) = self.phone.clipboard.clone() {
                ui.add_space(GAP);
                w::small(ui, th, &t("From the phone's clipboard:"));
                w::body(ui, th, &text);
                if w::button(ui, th, &t("Copy"), Some("copy"), Kind::Tonal, 36.0) { ui.ctx().copy_text(text); }
            }
        });
    }
}
