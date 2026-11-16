//! translation for * suite apps (= python `quezka_ui/i18n.py`): english text = key; table rows `(en, it, ru)`
//! lang: `<APP>_LANG` env -> chosen (`set_language`) -> system (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`) -> en
//! picked once at start (python same: change -> restart)

use std::sync::atomic::{AtomicU8, Ordering};

pub type Messages = &'static [(&'static str, &'static str, &'static str)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    It,
    Ru,
}

/// (code, name in own language); "" = follow system
pub const LANGUAGES: [(&str, &str); 4] = [("", "System"), ("en", "English"), ("it", "Italiano"), ("ru", "Русский")];

const UNSET: u8 = u8::MAX;
static ACTIVE: AtomicU8 = AtomicU8::new(UNSET);

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.get(..2) {
            Some("en") => Some(Lang::En),
            Some("it") => Some(Lang::It),
            Some("ru") => Some(Lang::Ru),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::It => "it",
            Lang::Ru => "ru",
        }
    }
}

/// system lang; it + ru get theirs, rest en
pub fn system_language(app_env: &str) -> Lang {
    [app_env, "LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| Lang::from_code(&v))
        .unwrap_or(Lang::En)
}

/// activate: env override > `chosen` code ("" = system)
pub fn init(app_env: &str, chosen: &str) -> Lang {
    let env = std::env::var(app_env).ok().and_then(|v| Lang::from_code(&v));
    let lang = env.or_else(|| Lang::from_code(chosen)).unwrap_or_else(|| system_language(app_env));
    set_language(lang);
    lang
}

pub fn set_language(lang: Lang) {
    ACTIVE.store(lang as u8, Ordering::Relaxed);
}

/// active lang; not init -> system w/o app override
pub fn language() -> Lang {
    match ACTIVE.load(Ordering::Relaxed) {
        1 => Lang::It,
        2 => Lang::Ru,
        0 => Lang::En,
        _ => {
            let l = system_language("QUEZKA_LANG");
            set_language(l);
            l
        }
    }
}

/// text in `lang`; missing row -> english
pub fn translate(messages: Messages, lang: Lang, english: &str) -> String {
    if lang == Lang::En {
        return english.to_string();
    }
    messages.iter().find(|(en, _, _)| *en == english)
        .map(|(_, it, ru)| if lang == Lang::It { *it } else { *ru })
        .unwrap_or(english).to_string()
}

/// `{name}` placeholders filled
pub fn fill(text: &str, values: &[(&str, &str)]) -> String {
    let mut text = text.to_string();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// for app tests: every `{x}` in en also in it + ru, none extra; no empty row
pub fn check_messages(messages: Messages) -> Result<(), String> {
    fn names(s: &str) -> Vec<&str> {
        let mut out: Vec<&str> = s.match_indices('{').filter_map(|(i, _)| s[i..].find('}').map(|j| &s[i..i + j + 1])).collect();
        out.sort_unstable();
        out
    }
    for (en, it, ru) in messages {
        if it.is_empty() || ru.is_empty() {
            return Err(format!("{en}: empty translation"));
        }
        if names(en) != names(it) || names(en) != names(ru) {
            return Err(format!("{en}: placeholders differ"));
        }
    }
    Ok(())
}

/// plural rows: (en singular, it one, it other, ru one, ru few, ru many)
pub type Plurals = &'static [(&'static str, &'static str, &'static str, &'static str, &'static str, &'static str)];

/// russian form: 1 урок (0), 2-4 урока (1), 5+ уроков (2); 11-14 = 5+
pub fn russian_form(n: i64) -> usize {
    let n = n.unsigned_abs();
    if n % 10 == 1 && n % 100 != 11 {
        0
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        1
    } else {
        2
    }
}

/// `3 lessons` | `3 урока`; `many` = whole en plural | ending (shorter than word); none -> `+s`
pub fn plural(plurals: Plurals, lang: Lang, n: i64, word: &str, many: Option<&str>) -> String {
    if let Some(row) = plurals.iter().find(|r| r.0 == word).filter(|_| lang != Lang::En) {
        let form = match lang {
            Lang::It => if n == 1 { row.1 } else { row.2 },
            _ => [row.3, row.4, row.5][russian_form(n)],
        };
        return format!("{n} {form}");
    }
    if n == 1 {
        return format!("{n} {word}");
    }
    match many {
        None => format!("{n} {word}s"),
        Some(m) if m.len() >= word.len() => format!("{n} {m}"),
        Some(m) => format!("{n} {word}{m}"),
    }
}

/// day + month names in one language
#[derive(Clone, Copy, Debug)]
pub struct Names {
    pub weekdays: [&'static str; 7],
    pub weekdays_short: [&'static str; 7],
    /// standalone: "September 2026"
    pub months: [&'static str; 12],
    /// after day number: "23 September" | "23 сентября"
    pub months_genitive: [&'static str; 12],
    pub months_short: [&'static str; 12],
}

pub const EN_NAMES: Names = Names {
    weekdays: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
    weekdays_short: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    months: ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    months_genitive: ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    months_short: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
};

/// app wrapper: `kit::translations!(MESSAGES);` -> `t(en)` + `tf(en, values)` in that module
#[macro_export]
macro_rules! translations {
    ($messages:expr) => {
        /// text in user's language (english when none)
        pub fn t(english: &str) -> String {
            $crate::i18n::translate($messages, $crate::i18n::language(), english)
        }
        /// `t` + `{name}` placeholders filled
        pub fn tf(english: &str, values: &[(&str, &str)]) -> String {
            $crate::i18n::fill(&t(english), values)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Messages = &[("Hello {name}", "Ciao {name}", "Привет {name}")];

    #[test]
    fn rows_and_fallbacks() {
        assert_eq!(translate(M, Lang::It, "Hello {name}"), "Ciao {name}");
        assert_eq!(translate(M, Lang::Ru, "Missing"), "Missing");
        assert_eq!(fill(&translate(M, Lang::En, "Hello {name}"), &[("name", "A")]), "Hello A");
        assert!(check_messages(M).is_ok());
        assert!(check_messages(&[("{n} left", "rimasti", "осталось {n}")]).is_err());
        assert_eq!(Lang::from_code("it_IT.UTF-8"), Some(Lang::It));
        assert_eq!(Lang::from_code("C"), None);
        const P: Plurals = &[("lesson", "lezione", "lezioni", "урок", "урока", "уроков")];
        assert_eq!(plural(P, Lang::Ru, 22, "lesson", None), "22 урока");
        assert_eq!(plural(P, Lang::Ru, 12, "lesson", None), "12 уроков");
        assert_eq!(plural(P, Lang::It, 1, "lesson", None), "1 lezione");
        assert_eq!(plural(P, Lang::En, 2, "class", Some("es")), "2 classes");
        assert_eq!(plural(P, Lang::En, 2, "entity", Some("entities")), "2 entities");
    }
}
