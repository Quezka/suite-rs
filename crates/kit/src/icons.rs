//! Material Symbols (Outlined, Apache 2.0, `assets/MaterialSymbols-LICENSE.txt`), drawn crisp at any size: each is
//! rendered once per size with resvg and kept as a texture. Outlined normally, filled when selected (Material 3).
//! The list is `symbols.rs`, written by `tools/make_symbols.py`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use egui::{Color32, Painter, Rect, TextureHandle, TextureOptions};

use crate::symbols::SYMBOLS;

type IconKey = (&'static str, bool, u32);

fn icon_cache() -> &'static Mutex<HashMap<IconKey, TextureHandle>> {
    static CACHE: OnceLock<Mutex<HashMap<IconKey, TextureHandle>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// White pixels of an icon at this size, which the painter then tints.
fn render_icon(name: &str, filled: bool, px: u32) -> Option<egui::ColorImage> {
    let (_, outlined, solid) = SYMBOLS.iter().find(|(n, _, _)| *n == name)?;
    let path = if filled { solid } else { outlined };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="#ffffff"><path d="{path}"/></svg>"##
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data()))
}

/// Draws the symbol `name` in `rect`, tinted `color`. Unknown names draw nothing.
pub fn icon(painter: &Painter, rect: Rect, name: &'static str, filled: bool, color: Color32) {
    let ctx = painter.ctx();
    let px = (rect.width() * ctx.pixels_per_point()).ceil().max(1.0) as u32;
    let Ok(mut cache) = icon_cache().lock() else {
        return;
    };
    let key = (name, filled, px);
    if !cache.contains_key(&key) {
        let Some(image) = render_icon(name, filled, px) else {
            return;
        };
        let texture = ctx.load_texture(format!("icon-{name}-{filled}-{px}"), image, TextureOptions::LINEAR);
        cache.insert(key, texture);
    }
    if let Some(texture) = cache.get(&key) {
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture.id(), rect, uv, color);
    }
}

type PictureKey = (&'static str, u32);

fn picture_cache() -> &'static Mutex<HashMap<PictureKey, Option<TextureHandle>>> {
    static CACHE: OnceLock<Mutex<HashMap<PictureKey, Option<TextureHandle>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A full-colour SVG (an app's own icon, say) as pixels `px` wide, keeping its aspect ratio inside a square.
pub fn render_svg(svg: &[u8], px: u32) -> Option<egui::ColorImage> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    let (dx, dy) = ((px as f32 - size.width() * scale) / 2.0, (px as f32 - size.height() * scale) / 2.0);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy), &mut pixmap.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data()))
}

/// Draws a full-colour SVG in `rect`, crisp at the screen's scale; `key` names it in the texture cache. Returns
/// false if the SVG couldn't be read (draw something else then).
pub fn picture(painter: &Painter, rect: Rect, key: &'static str, svg: &'static [u8]) -> bool {
    let ctx = painter.ctx();
    let px = (rect.width().max(rect.height()) * ctx.pixels_per_point()).ceil().max(1.0) as u32;
    let Ok(mut cache) = picture_cache().lock() else {
        return false;
    };
    let texture = cache
        .entry((key, px))
        .or_insert_with(|| render_svg(svg, px).map(|image| ctx.load_texture(format!("picture-{key}-{px}"), image, TextureOptions::LINEAR)));
    match texture {
        Some(texture) => {
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), rect, uv, Color32::WHITE);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_renders() {
        for (name, _, _) in SYMBOLS {
            assert!(render_icon(name, false, 24).is_some() && render_icon(name, true, 24).is_some(), "{name}");
        }
    }
}
