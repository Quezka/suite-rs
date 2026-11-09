//! The suite's typeface, Manrope (SIL Open Font License, `assets/Manrope-OFL.txt`), in four weights, with Cyrillic
//! for the Russian translations. egui's own fonts stay behind it for anything Manrope lacks (emoji, symbols).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
    Bold,
}

impl Weight {
    pub fn family(self) -> FontFamily {
        match self {
            Self::Regular => FontFamily::Proportional,
            Self::Medium => FontFamily::Name("medium".into()),
            Self::SemiBold => FontFamily::Name("semibold".into()),
            Self::Bold => FontFamily::Name("bold".into()),
        }
    }

    pub fn font(self, size: f32) -> FontId {
        FontId::new(size, self.family())
    }
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let faces: [(&str, &'static [u8], Weight); 4] = [
        ("manrope-regular", include_bytes!("../assets/Manrope-Regular.ttf"), Weight::Regular),
        ("manrope-medium", include_bytes!("../assets/Manrope-Medium.ttf"), Weight::Medium),
        ("manrope-semibold", include_bytes!("../assets/Manrope-SemiBold.ttf"), Weight::SemiBold),
        ("manrope-bold", include_bytes!("../assets/Manrope-Bold.ttf"), Weight::Bold),
    ];
    for (name, bytes, weight) in faces {
        fonts.font_data.insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        fonts.families.insert(weight.family(), list);
    }
    ctx.set_fonts(fonts);
}
