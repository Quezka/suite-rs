//! Clean Architecture: the domain knows nothing outside itself (no storage, network, screens or other layers).

#[test]
fn the_domain_depends_on_nothing_outside_it() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for forbidden in ["crate::application", "crate::infrastructure", "crate::presentation", "rusqlite", "egui", "quezka_kit", "std::fs", "std::net"] {
            assert!(!text.contains(forbidden), "{} uses {forbidden}", path.display());
        }
    }
}

#[test]
fn application_and_screens_never_reach_into_infrastructure() {
    for layer in ["src/application", "src/presentation"] {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(layer);
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            for forbidden in ["crate::infrastructure", "rusqlite::", "ureq::", "keyring::", "crate::bootstrap"] {
                assert!(!text.contains(forbidden), "{} uses {forbidden}", path.display());
            }
            if layer == "src/application" {
                for forbidden in ["crate::presentation", "egui::", "quezka_kit::"] {
                    assert!(!text.contains(forbidden), "{} uses {forbidden}", path.display());
                }
            }
        }
    }
}
