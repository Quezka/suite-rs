//! One window, three cards: send this camera, receive a camera, second screen.

use std::time::{Duration, Instant};

use quezka_kit::fonts::Weight;
use quezka_kit::theme::GAP;
use quezka_kit::widgets::{self as w, Kind};
use quezka_kit::{App, Tokens};

use super::i18n::{t, tf};
use crate::application::LoupeService;
use crate::domain::Peer;

pub struct LoupeWindow {
    service: LoupeService,
    peers: Vec<Peer>,
    last_tick: Option<Instant>,
    message: String,
    code: String,
}

impl LoupeWindow {
    pub fn new(service: LoupeService) -> Self {
        Self { service, peers: Vec::new(), last_tick: None, message: String::new(), code: String::new() }
    }

    pub fn shutdown(&mut self) {
        self.service.shutdown();
    }

    fn act(&mut self, result: Result<(), crate::application::LoupeError>) {
        self.message = result.err().map(|e| t(&e.0)).unwrap_or_default();
    }
}

impl App for LoupeWindow {
    fn ui(&mut self, root: &mut egui::Ui, th: &Tokens) {
        if self.last_tick.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
            self.peers = self.service.tick();
            self.last_tick = Some(Instant::now());
        }
        root.ctx().request_repaint_after(Duration::from_millis(1000));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg).inner_margin(egui::Margin::same(GAP as i8 * 2)))
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(w::text("Loupe", 28.0, th.text, Weight::Bold));
                    ui.label(w::text(format!("{} · {}", self.service.name(), t("Camera and second screen over your home network")),
                        15.0, th.muted, Weight::Regular));
                    ui.add_space(GAP * 1.5);
                    if !self.message.is_empty() {
                        w::banner(ui, th, "alert", &self.message, "", None);
                        ui.add_space(GAP);
                    }
                    self.camera_card(ui, th);
                    ui.add_space(GAP);
                    self.receive_card(ui, th);
                    ui.add_space(GAP);
                    self.screen_card(ui, th);
                });
            });
    }
}

impl LoupeWindow {
    fn camera_card(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        let peers = self.peers.clone();
        let sending = self.service.sending_to().map(str::to_string);
        let has_camera = self.service.has_camera();
        w::settings_card(ui, th, Some(&t("Send this camera")), |ui| {
            w::body(ui, th, &t("This computer's camera shows up on the other one as a normal webcam."));
            if !has_camera {
                w::small(ui, th, &t("This computer has no camera."));
                return;
            }
            if let Some(host) = &sending {
                let name = peers.iter().find(|p| &p.address == host).map(|p| p.name.clone()).unwrap_or(host.clone());
                ui.label(w::text(tf("Sending to {name}", &[("name", &name)]), 17.0, th.accent, Weight::Bold));
                if w::button(ui, th, &t("Stop"), Some("pause"), Kind::Tonal, 44.0) {
                    self.service.stop_sending();
                }
                return;
            }
            if peers.is_empty() {
                w::small(ui, th, &t("No other computer with Loupe found yet. Open Loupe on it too."));
            }
            for p in &peers {
                ui.horizontal(|ui| {
                    ui.label(w::text(format!("{}  ({})", p.name, p.address), 16.0, th.text, Weight::Medium));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::button(ui, th, &t("Send"), Some("send"), Kind::Filled, 40.0) {
                            let r = self.service.send_camera(&p.address);
                            self.act(r);
                        }
                    });
                });
            }
        });
    }

    fn receive_card(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        let on = self.service.receiving();
        let mut toggle = false;
        w::settings_card(ui, th, Some(&t("Receive a camera")), |ui| {
            toggle = w::switch_row(ui, th, &t("Show other computers' cameras here as a webcam"), on);
            w::small(ui, th, &t("Apps see it as \"Loupe camera\". The first time asks for your password (to add the virtual webcam)."));
        });
        if toggle {
            if on { self.service.stop_receiving() } else { let r = self.service.receive_camera(); self.act(r); }
        }
    }

    fn screen_card(&mut self, ui: &mut egui::Ui, th: &Tokens) {
        let sharing = self.service.screen_code.clone();
        let viewing = self.service.viewing();
        let hosts: Vec<Peer> = self.peers.iter().filter(|p| p.shares_screen).cloned().collect();
        let mut toggle = false;
        let mut connect: Option<String> = None;
        let mut stop_view = false;
        let code = &mut self.code;
        w::settings_card(ui, th, Some(&t("Second screen")), |ui| {
            toggle = w::switch_row(ui, th, &t("Let a laptop extend this screen"), sharing.is_some());
            if let Some(c) = &sharing {
                w::small(ui, th, &t("On the laptop, open Loupe, pick this computer and type this code:"));
                ui.label(w::text(c, 32.0, th.accent, Weight::Bold));
            }
            ui.add_space(GAP);
            ui.label(w::text(t("Be the second screen of"), 17.0, th.text, Weight::Bold));
            if viewing {
                w::small(ui, th, &t("Showing the other screen. Leave full screen with Ctrl+Alt+Enter."));
                stop_view = w::button(ui, th, &t("Stop"), Some("pause"), Kind::Tonal, 44.0);
                return;
            }
            if hosts.is_empty() {
                w::small(ui, th, &t("No computer is sharing its screen yet."));
            }
            for p in &hosts {
                ui.horizontal(|ui| {
                    ui.label(w::text(&p.name, 16.0, th.text, Weight::Medium));
                    ui.add(egui::TextEdit::singleline(code).hint_text(t("Code")).desired_width(120.0));
                    if w::button(ui, th, &t("Connect"), Some("play"), Kind::Filled, 40.0) {
                        connect = Some(p.address.clone());
                    }
                });
            }
        });
        if toggle {
            let r = self.service.share_screen(sharing.is_none());
            self.act(r);
        }
        if stop_view {
            self.service.stop_viewing();
        }
        if let Some(host) = connect {
            let code = self.code.clone();
            let r = self.service.view_screen(&host, &code);
            self.act(r);
        }
    }
}
