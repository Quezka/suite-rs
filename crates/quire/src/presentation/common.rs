//! bits every quire screen uses: change counter (reload only after writes), page header, notice line

use std::cell::Cell;
use std::rc::Rc;

use egui::Ui;
use quezka_kit::fonts::Weight;
use quezka_kit::widgets as w;
use quezka_kit::Tokens;

use crate::application::bus::ChangeBus;

/// counts bus publishes -> screens cache data per count (no sqlite query per frame)
#[derive(Clone, Default)]
pub struct Changes(Rc<Cell<u64>>);

impl Changes {
    pub fn watch(bus: &ChangeBus) -> Self {
        let c = Changes::default();
        let inner = c.0.clone();
        bus.subscribe(move |_| inner.set(inner.get() + 1));
        c
    }
    pub fn count(&self) -> u64 {
        self.0.get()
    }
    /// force reload (day changed, clock minute)
    pub fn bump(&self) {
        self.0.set(self.0.get() + 1);
    }
}

pub fn page_title(ui: &mut Ui, th: &Tokens, title: &str, subtitle: &str) {
    ui.label(w::text(title, 26.0, th.text, Weight::Bold));
    if !subtitle.is_empty() {
        ui.label(w::text(subtitle, 15.0, th.muted, Weight::Medium));
    }
}

/// one line: what happened (+ undo button outside); empty = hidden
#[derive(Default)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

impl Notice {
    pub fn say(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.error = false;
    }
    pub fn fail(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.error = true;
    }
    /// draws line; true = user closed it
    pub fn ui(&mut self, ui: &mut Ui, th: &Tokens) {
        if self.text.is_empty() {
            return;
        }
        ui.horizontal(|ui| {
            let ink = if self.error { th.danger } else { th.muted };
            ui.label(w::text(&self.text, 15.0, ink, Weight::Medium));
            if quezka_kit::forms::inline_button(ui, th, &super::i18n::t("OK"), w::Kind::Text) {
                self.text.clear();
            }
        });
    }
}
