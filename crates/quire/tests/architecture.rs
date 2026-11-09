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
