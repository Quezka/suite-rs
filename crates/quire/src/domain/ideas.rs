//! Ideas caught in a second, looked at again later (port of `quire/domain/ideas.py`; same rules as `Ideas.kt`).

use chrono::{Duration, NaiveDate, NaiveDateTime};
use fancy_regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

use super::errors::{Checked, ValidationError};

pub const MAX_IDEA_CHARS: usize = 2000;
pub const RESURFACE_AFTER_DAYS: i64 = 7; // an open idea this old may come back in the digest
pub const RESURFACE_GAP_DAYS: i64 = 14; // and not again sooner than this

#[derive(Clone, Debug, PartialEq)]
pub struct Idea {
    pub text: String,
    pub created: NaiveDateTime,
    pub done: bool,
    pub id: Option<i64>,
}

impl Idea {
    pub fn validate(&self) -> Checked {
        if self.text.trim().is_empty() {
            return Err(ValidationError("Write the idea first."));
        }
        if self.text.chars().count() > MAX_IDEA_CHARS {
            return Err(ValidationError("That's long for an idea: make it a note instead."));
        }
        Ok(())
    }
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?<![\w#])#(\w[\w-]*)").expect("valid"))
}

/// `#school`, `#game`: tags are words in the text, no separate field.
pub fn tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in tag_re().captures_iter(text).flatten() {
        let t = c[1].to_lowercase();
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Every word of the query is in the text (case-insensitive); `#tag` words must be tags.
pub fn matches(idea: &Idea, query: &str) -> bool {
    let text = idea.text.to_lowercase();
    let own = tags(&idea.text);
    query.to_lowercase().split_whitespace().all(|w| match w.strip_prefix('#') {
        Some(t) => own.iter().any(|o| o == t),
        None => text.contains(w),
    })
}

/// Ideas caught between two days (both included), oldest first.
pub fn new_on(ideas: &[Idea], first: NaiveDate, last: NaiveDate) -> Vec<Idea> {
    let mut out: Vec<Idea> = ideas.iter().filter(|i| (first..=last).contains(&i.created.date())).cloned().collect();
    out.sort_by_key(|i| i.created);
    out
}

/// One open idea to look at again: older than a week, not shown in the last two, the longest unseen first.
/// `seen` maps str(id) to the ISO day it last came back.
pub fn resurface(ideas: &[Idea], today: NaiveDate, seen: &HashMap<String, String>) -> Option<Idea> {
    let last = |i: &Idea| i.id.and_then(|id| seen.get(&id.to_string())).and_then(|d| d.parse::<NaiveDate>().ok());
    ideas
        .iter()
        .filter(|i| {
            !i.done
                && i.created.date() <= today - Duration::days(RESURFACE_AFTER_DAYS)
                && last(i).is_none_or(|d| d <= today - Duration::days(RESURFACE_GAP_DAYS))
        })
        .min_by_key(|i| (last(i).unwrap_or(NaiveDate::MIN), i.created))
        .cloned()
}

/// A task title from an idea: its first line without tags, cut at a word.
pub fn first_line(text: &str, limit: usize) -> String {
    let first = text.trim().lines().next().unwrap_or("");
    let line = tag_re().replace_all(first, "").split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        return line;
    }
    let cut: String = line.chars().take(limit).collect();
    match cut.rfind(' ') {
        Some(i) => format!("{}…", &cut[..i]),
        None => format!("{cut}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: &str) -> NaiveDateTime {
        format!("{d}T09:00:00").parse().unwrap()
    }

    #[test]
    fn same_answers_as_python() {
        assert_eq!(tags("A shelf for the #3DS games #home #Home"), vec!["3ds", "home"]);
        assert_eq!(tags("issue #12 and a#b"), vec!["12"]);
        let idea = Idea { text: "Ask Sam about the #school trip".into(), created: at("2026-11-01"), done: false, id: Some(1) };
        assert!(matches(&idea, "sam trip") && matches(&idea, "#school") && !matches(&idea, "#game") && matches(&idea, ""));
        let today: NaiveDate = "2026-11-15".parse().unwrap();
        let old = Idea { text: "old".into(), created: at("2026-11-01"), done: false, id: Some(1) };
        let older = Idea { text: "older".into(), created: at("2026-10-01"), done: false, id: Some(2) };
        let fresh = Idea { text: "fresh".into(), created: at("2026-11-14"), done: false, id: Some(3) };
        let all = vec![old.clone(), older.clone(), fresh];
        assert_eq!(resurface(&all, today, &HashMap::new()).unwrap().id, Some(2));
        let seen: HashMap<String, String> = [("2".to_string(), "2026-11-10".to_string())].into();
        assert_eq!(resurface(&[old, older.clone()], today, &seen).unwrap().id, Some(1));
        let seen: HashMap<String, String> = [("2".to_string(), "2026-11-12".to_string())].into();
        assert!(resurface(&[older], today, &seen).is_none());
        assert_eq!(first_line("Buy #home paint\nwhite, 2 l", 120), "Buy paint");
        assert!(first_line(&"word ".repeat(50), 20).ends_with('…'));
    }
}
