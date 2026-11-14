//! Each app's real icon, bundled (the same SVG its own package installs), so it shows on Linux and Windows and
//! for apps that aren't installed yet. Refresh with `tools/quay_icons.sh`.

pub fn icon(key: &str) -> Option<&'static [u8]> {
    Some(match key {
        "quire" => include_bytes!("../../assets/apps/quire.svg"),
        "gantry" => include_bytes!("../../assets/apps/gantry.svg"),
        "ligature" => include_bytes!("../../assets/apps/ligature.svg"),
        "folio" => include_bytes!("../../assets/apps/folio.svg"),
        "bivouac" => include_bytes!("../../assets/apps/bivouac.svg"),
        "facet" => include_bytes!("../../assets/apps/facet.svg"),
        "missive" => include_bytes!("../../assets/apps/missive.svg"),
        "stagno" => include_bytes!("../../assets/apps/stagno.svg"),
        "natter" => include_bytes!("../../assets/apps/natter.svg"),
        "quay" => include_bytes!("../../assets/apps/quay.svg"),
        "loupe" => include_bytes!("../../assets/apps/loupe.svg"),
        "tether" => include_bytes!("../../assets/apps/tether.svg"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_app_has_an_icon_that_renders() {
        for app in crate::domain::catalog::CATALOG {
            let svg = super::icon(app.key).unwrap_or_else(|| panic!("{} has no icon", app.key));
            assert!(quezka_kit::icons::render_svg(svg, 64).is_some(), "{}'s icon doesn't render", app.key);
        }
    }
}
