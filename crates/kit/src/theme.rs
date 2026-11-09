//! The suite's colours and shape, token for token the same as the Python kit (`quezka_ui/theme.py`) and the phone
//! apps, so the Rust apps look like the rest of the suite while they replace it.

use egui::Color32;

/// Every outer surface (sidebar, cards, sheets, floating bars) shares this radius.
pub const OUTER_RADIUS: f32 = crate::tokens::RADIUS_LG;
/// The space between outer surfaces, and between them and the window's edge.
pub const GAP: f32 = crate::tokens::GAP;
/// Padding inside a card; things inside it touching that padding get `OUTER_RADIUS - CARD_PADDING`.
pub const CARD_PADDING: f32 = crate::tokens::SPACE_3;

/// The radius for something sitting `padding` inside a surface of radius `outer`, so the two curves run side by side.
pub fn nested(outer: f32, padding: f32) -> f32 {
    (outer - padding).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub bg: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub sidebar: Color32,
    pub border: Color32,
    pub hover: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub on_accent_soft: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub dark: bool,
    /// High contrast: solid everything, stark outlines, no glass.
    pub high_contrast: bool,
}

pub use crate::tokens::{DARK, HIGH_DARK, HIGH_LIGHT, LIGHT};

pub fn tokens(dark: bool, high_contrast: bool) -> Tokens {
    match (dark, high_contrast) {
        (false, false) => LIGHT,
        (true, false) => DARK,
        (false, true) => HIGH_LIGHT,
        (true, true) => HIGH_DARK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_nest() {
        assert_eq!(nested(OUTER_RADIUS, CARD_PADDING), 8.0);
        assert_eq!(nested(8.0, 12.0), 0.0);
    }

    #[test]
    fn high_contrast_is_black_and_white() {
        assert_eq!(tokens(false, true).text, Color32::BLACK);
        assert_eq!(tokens(true, true).bg, Color32::BLACK);
        assert!(tokens(true, false).dark && !tokens(false, false).dark);
    }
}
