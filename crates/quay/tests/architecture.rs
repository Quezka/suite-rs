//! The layers point inward: domain knows nothing else, application only the domain, infrastructure and
//! presentation not each other, and nothing below presentation draws.

use std::path::Path;

fn sources(layer: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(layer);
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or("").to_string();
            (p.display().to_string(), code)
        })
        .collect()
}

fn forbid(layer: &str, words: &[&str]) {
    for (file, code) in sources(layer) {
        for word in words {
            assert!(!code.contains(word), "{file} mentions {word}");
        }
    }
}

#[test]
fn domain_is_pure() {
    forbid("domain", &["crate::application", "crate::infrastructure", "crate::presentation", "egui", "std::process", "std::fs"]);
}

#[test]
fn application_knows_only_the_domain() {
    forbid("application", &["crate::infrastructure", "crate::presentation", "egui", "std::process", "serde_json"]);
}

#[test]
fn outer_layers_stay_apart() {
    forbid("infrastructure", &["crate::presentation", "egui::"]);
    forbid("presentation", &["crate::infrastructure", "std::process"]);
}
