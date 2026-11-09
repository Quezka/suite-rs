//! The suite's colours and shape, token for token the same as the Python kit (`quezka_ui/theme.py`) and the phone
//! apps, so the Rust apps look like the rest of the suite while they replace it.

use egui::Color32;

/// Every outer surface (sidebar, cards, sheets, floating bars) shares this radius.
pub const OUTER_RADIUS: f32 = 20.0;
/// The space between outer surfaces, and between them and the window's edge.
pub const GAP: f32 = 10.0;
/// Padding inside a card; things inside it touching that padding get `OUTER_RADIUS - CARD_PADDING`.
pub const CARD_PADDING: f32 = 12.0;

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
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
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

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Tokens = Tokens {
    bg: hex(0xf3f2ff),
    surface: hex(0xffffff),
    raised: hex(0xe9e8ff),
    sidebar: hex(0xeceaff),
    border: hex(0xb3b6cc),
    text: hex(0x0b0d14),
    muted: hex(0x343b4d),
    faint: hex(0x4d5466),
    accent: hex(0x4f46e5),
    accent_soft: hex(0xdcdbff),
    on_accent: hex(0xffffff),
    on_accent_soft: hex(0x1e1a78),
    danger: hex(0xd0262d),
    success: hex(0x127a46),
    warning: hex(0xb45309),
    dark: false,
    high_contrast: false,
};

pub const DARK: Tokens = Tokens {
    bg: hex(0x0e0e19),
    surface: hex(0x1a1a2b),
    raised: hex(0x27273f),
    sidebar: hex(0x13131f),
    border: hex(0x4b4e66),
    text: hex(0xf6f7fb),
    muted: hex(0xcbcfdd),
    faint: hex(0xa2a8bb),
    accent: hex(0xa9a6ff),
    accent_soft: hex(0x3b3984),
    on_accent: hex(0x0b0b14),
    on_accent_soft: hex(0xe4e3ff),
    danger: hex(0xff7b80),
    success: hex(0x4ade80),
    warning: hex(0xfbbf24),
    dark: true,
    high_contrast: false,
};

pub const HIGH_LIGHT: Tokens = Tokens {
    bg: hex(0xffffff),
    surface: hex(0xffffff),
    raised: hex(0xf0f0f0),
    sidebar: hex(0xf0f0f0),
    border: hex(0x000000),
    text: hex(0x000000),
    muted: hex(0x1a1a1a),
    faint: hex(0x333333),
    accent: hex(0x1f1fa8),
    accent_soft: hex(0xcfcff7),
    on_accent: hex(0xffffff),
    on_accent_soft: hex(0x000000),
    danger: hex(0xa30000),
    success: hex(0x005a2b),
    warning: hex(0x7a4100),
    dark: false,
    high_contrast: true,
};

pub const HIGH_DARK: Tokens = Tokens {
    bg: hex(0x000000),
    surface: hex(0x000000),
    raised: hex(0x1a1a1a),
    sidebar: hex(0x0a0a0a),
    border: hex(0xffffff),
    text: hex(0xffffff),
    muted: hex(0xe6e6e6),
    faint: hex(0xcccccc),
    accent: hex(0xffd400),
    accent_soft: hex(0x3a3300),
    on_accent: hex(0x000000),
    on_accent_soft: hex(0xffffff),
    danger: hex(0xff8a8a),
    success: hex(0x6dffb0),
    warning: hex(0xffd400),
    dark: true,
    high_contrast: true,
};

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
