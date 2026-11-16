//! Layers point inward, as in every suite app.
use std::path::Path;

fn code(layer: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(layer);
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| { let t = std::fs::read_to_string(&p).unwrap(); (p.display().to_string(), t.split("#[cfg(test)]").next().unwrap_or("").into()) }).collect()
}
fn forbid(layer: &str, words: &[&str]) {
    for (file, c) in code(layer) { for w in words { assert!(!c.contains(w), "{file} mentions {w}"); } }
}
#[test] fn domain_is_pure() { forbid("domain", &["crate::application", "crate::infrastructure", "crate::presentation", "egui", "std::process", "std::fs", "roxmltree"]); }
#[test] fn application_knows_only_the_domain() { forbid("application", &["crate::infrastructure", "crate::presentation", "egui", "std::process", "roxmltree", "serde_json"]); }
#[test] fn outer_layers_stay_apart() { forbid("infrastructure", &["crate::presentation", "egui::"]); forbid("presentation", &["crate::infrastructure", "std::process", "roxmltree"]); }
