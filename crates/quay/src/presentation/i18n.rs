//! Translation: the language comes from the system (LANGUAGE, LC_ALL, LC_MESSAGES, LANG); English otherwise.

use std::sync::OnceLock;

use super::messages::MESSAGES;

fn language() -> usize {
    static LANG: OnceLock<usize> = OnceLock::new();
    *LANG.get_or_init(|| {
        let wanted = ["QUAY_LANG", "LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        match wanted.get(..2) {
            Some("it") => 1,
            Some("ru") => 2,
            _ => 0,
        }
    })
}

/// The text in the user's language (English when there's no translation).
pub fn t(english: &str) -> String {
    let lang = language();
    if lang == 0 {
        return english.to_string();
    }
    MESSAGES
        .iter()
        .find(|(en, _, _)| *en == english)
        .map(|(_, it, ru)| if lang == 1 { *it } else { *ru })
        .unwrap_or(english)
        .to_string()
}

/// `t` with `{name}` placeholders filled in.
pub fn tf(english: &str, values: &[(&str, &str)]) -> String {
    let mut text = t(english);
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::super::messages::MESSAGES;

    #[test]
    fn placeholders_survive_translation() {
        for (en, it, ru) in MESSAGES {
            for name in ["{n}", "{name}", "{error}", "{latest}", "{installed}", "{done}", "{total}", "{names}"] {
                let want = en.contains(name);
                assert_eq!(it.contains(name), want, "{en} (it)");
                assert_eq!(ru.contains(name), want, "{en} (ru)");
            }
        }
    }
}
