//! One window: the phone's status and pairing, its notifications, its messages, files and clipboard.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use quezka_kit::fonts::Weight;
use quezka_kit::theme::GAP;
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::{App, Tokens};

use super::i18n::{t, tf};
use crate::application::{to_phone, Alert, Call, Desktop, LinkEvent, Phone, PhoneLink};

pub struct TetherWindow {
    link: Box<dyn PhoneLink>,
    desktop: Box<dyn Desktop>,
    events: Receiver<LinkEvent>,
    phone: Phone,
    tab: usize,
    code: Option<String>,
    replies: std::collections::HashMap<String, String>,
    thread: Option<String>,
    draft: String,
    message: String,
}

impl TetherWindow {
    pub fn new(link: Box<dyn PhoneLink>, desktop: Box<dyn Desktop>, events: Receiver<LinkEvent>) -> Self {
        Self { link, desktop, events, phone: Phone::default(), tab: 0, code: None, replies: Default::default(), thread: None,
            draft: String::new(), message: String::new() }
    }

    fn pump(&mut self, ctx: &egui::Context) {
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
}

impl App for TetherWindow {
    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        let ctx = root.ctx().clone();
        self.pump(&ctx);
        ctx.request_repaint_after(Duration::from_millis(300));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg).inner_margin(egui::Margin::same(GAP as i8 * 2)))
            .show(root, |ui| {
                ui.label(w::text("Tether", 28.0, th.text, Weight::Bold));
                let status = if self.phone.connected { tf("Connected to {name}", &[("name", &self.phone.name)]) } else { t("Your phone on this computer") };
                ui.label(w::text(status, 15.0, th.muted, Weight::Regular));
                ui.add_space(GAP);
                if !self.message.is_empty() {
                    w::small(ui, th, &self.message);
                }
                self.call_banner(ui, th);
                let tabs = [t("Phone"), t("Notifications"), t("Messages"), t("Files")];
                let names: Vec<&str> = tabs.iter().map(String::as_str).collect();
                if let Some(i) = w::segmented(ui, th, &names, self.tab) {
                    self.tab = i;
                }
                ui.add_space(GAP);
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    0 => self.phone_tab(ui, th),
                    1 => self.notifications_tab(ui, th),
                    2 => self.messages_tab(ui, th),
                    _ => self.files_tab(ui, th),
                });
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
                ui.add_space(GAP);
            }
            Some(Call::Talking { number, name }) => {
                let who = if name.is_empty() { number } else { name };
                w::card(ui, th, |ui| {
                    ui.label(w::text(tf("On a call with {name}", &[("name", &who)]), 18.0, th.text, Weight::Bold));
                    if w::button(ui, th, &t("Hang up"), None, Kind::Tonal, 44.0) { self.link.send(to_phone::reject()); }
                });
                ui.add_space(GAP);
            }
            _ => {}
        }
    }

    fn phone_tab(&mut self, ui: &mut egui::Ui, th: &Tokens) {
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

    fn notifications_tab(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        if self.phone.notifications.is_empty() {
            w::body(ui, th, &t("Nothing new on the phone."));
        }
        let mut actions: Vec<serde_json::Value> = Vec::new();
        for n in self.phone.notifications.clone() {
            w::card(ui, th, |ui| {
                ui.label(w::text(&n.app, 13.0, th.muted, Weight::Bold));
                ui.label(w::text(&n.title, 17.0, th.text, Weight::Bold));
                if !n.text.is_empty() { w::body(ui, th, &n.text); }
                ui.horizontal(|ui| {
                    if n.can_reply {
                        let draft = self.replies.entry(n.key.clone()).or_default();
                        ui.add(egui::TextEdit::singleline(draft).hint_text(t("Reply…")).desired_width(260.0));
                        if w::button(ui, th, &t("Send"), Some("send"), Kind::Filled, 36.0) && !draft.trim().is_empty() {
                            actions.push(to_phone::reply(&n.key, draft.trim()));
                            draft.clear();
                        }
                    }
                    if w::button(ui, th, &t("Dismiss"), None, Kind::Tonal, 36.0) {
                        actions.push(to_phone::dismiss(&n.key));
                    }
                });
            });
            ui.add_space(GAP / 2.0);
        }
        for a in actions {
            if a["t"] == "dismiss" {
                let key = a["key"].as_str().unwrap_or_default().to_string();
                self.phone.notifications.retain(|n| n.key != key);
            }
            self.send(a);
        }
    }

    fn messages_tab(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        if w::button(ui, th, &t("Load messages"), Some("refresh"), Kind::Tonal, 36.0) {
            self.send(to_phone::sms_get());
        }
        ui.add_space(GAP);
        let threads = self.phone.threads.clone();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                for th_ in &threads {
                    let name = if th_.name.is_empty() { th_.address.clone() } else { th_.name.clone() };
                    let chosen = self.thread.as_deref() == Some(th_.address.as_str());
                    if w::filter_chip(ui, th, &name, chosen) {
                        self.thread = Some(th_.address.clone());
                    }
                }
            });
            ui.vertical(|ui| {
                let Some(address) = self.thread.clone() else { return };
                if let Some(thread) = threads.iter().find(|x| x.address == address) {
                    for m in thread.messages.iter().rev().take(40).rev() {
                        let color = if m.incoming { th.text } else { th.accent };
                        ui.label(w::text(&m.body, 16.0, color, if m.incoming { Weight::Regular } else { Weight::Medium }));
                        ui.add_space(4.0);
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.draft).hint_text(t("Write a message…")).desired_rows(2).desired_width(320.0));
                    if w::button(ui, th, &t("Send"), Some("send"), Kind::Filled, 40.0) && !self.draft.trim().is_empty() {
                        let body = self.draft.trim().to_string();
                        self.draft.clear();
                        self.phone.add_sms(&address, "", crate::application::Sms { incoming: false, body: body.clone(), at: i64::MAX });
                        self.send(to_phone::sms_send(&address, &body));
                    }
                });
            });
        });
    }

    fn files_tab(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        w::settings_card(ui, th, Some(&t("Files")), |ui| {
            if w::button(ui, th, &t("Send a file to the phone"), Some("upload"), Kind::Filled, 44.0) {
                if let Some(path) = self.desktop.pick_file() {
                    if let Err(e) = self.link.send_file(&path) { self.message = t(&e); }
                }
            }
            w::small(ui, th, &t("Files the phone shares with Tether land in your Downloads folder."));
            ui.add_space(GAP);
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
