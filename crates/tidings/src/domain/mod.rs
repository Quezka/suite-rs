//! Pure rules: sources and their lean, grouping articles into stories, who covers a story, blindspots,
//! stock mentions, feed dates.

use std::collections::{BTreeSet, HashMap};

/// Where a source sits. The user's own ratings (Sources page), started from common public ratings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lean {
    Left,
    CenterLeft,
    Center,
    CenterRight,
    Right,
    Unrated,
}

impl Lean {
    pub const ALL: [Lean; 6] = [Lean::Left, Lean::CenterLeft, Lean::Center, Lean::CenterRight, Lean::Right, Lean::Unrated];

    pub fn code(self) -> &'static str {
        match self {
            Lean::Left => "left",
            Lean::CenterLeft => "center-left",
            Lean::Center => "center",
            Lean::CenterRight => "center-right",
            Lean::Right => "right",
            Lean::Unrated => "unrated",
        }
    }

    pub fn from_code(code: &str) -> Lean {
        Lean::ALL.into_iter().find(|l| l.code() == code).unwrap_or(Lean::Unrated)
    }

    /// The three bands a coverage bar shows.
    pub fn side(self) -> Option<Side> {
        match self {
            Lean::Left | Lean::CenterLeft => Some(Side::Left),
            Lean::Center => Some(Side::Center),
            Lean::CenterRight | Lean::Right => Some(Side::Right),
            Lean::Unrated => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub feed: String,
    pub lean: Lean,
    /// "en", "it": stories only group within one language
    pub language: String,
    pub on: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Article {
    pub source: String,
    pub title: String,
    pub link: String,
    /// unix seconds; 0 = unknown
    pub published: i64,
    pub summary: String,
}

/// One story: articles from several sources about the same thing.
#[derive(Clone, Debug)]
pub struct Story {
    /// indexes into the article list, newest first
    pub articles: Vec<usize>,
    pub title: String,
    pub newest: i64,
    pub left: usize,
    pub center: usize,
    pub right: usize,
    pub sources: usize,
}

impl Story {
    /// One side covers it (2+ sources), the other not at all.
    pub fn blindspot(&self) -> Option<Side> {
        if self.left >= 2 && self.right == 0 {
            Some(Side::Right) // the right doesn't see it
        } else if self.right >= 2 && self.left == 0 {
            Some(Side::Left)
        } else {
            None
        }
    }
}

const STOP: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "after", "over", "into", "about", "says", "said", "will",
    "has", "have", "are", "was", "were", "its", "his", "her", "their", "new", "how", "why", "what", "who", "not",
    "but", "you", "our", "more", "than", "amid", "as", "at", "by", "on", "in", "of", "to", "a", "an", "is", "be",
    "della", "delle", "degli", "dello", "nella", "nelle", "sulla", "sulle", "alla", "alle", "dopo", "come", "anche",
    "per", "con", "una", "uno", "gli", "che", "non", "sono", "del", "dei", "nel", "sul", "tra", "fra", "più", "suo",
    "sua", "loro", "questo", "questa", "ecco", "ora", "oggi",
];

/// Words that say what a headline is about: lower case, no punctuation, no filler, 3+ letters.
pub fn words(title: &str) -> BTreeSet<String> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.as_str()))
        .map(|w| if w.len() > 5 && w.ends_with('s') { w[..w.len() - 1].to_string() } else { w })
        .collect()
}

/// Shared words / all words (Jaccard), 0..1.
pub fn similarity(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(b).count() as f64;
    shared / (a.len() + b.len()) as f64 * 2.0 // dice: kinder to short headlines than jaccard
}

pub const SAME_STORY: f64 = 0.42;

/// Groups articles into stories (same language only), newest stories first.
pub fn stories(articles: &[Article], sources: &[Source]) -> Vec<Story> {
    let by_id: HashMap<&str, &Source> = sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut order: Vec<usize> = (0..articles.len()).filter(|&i| by_id.get(articles[i].source.as_str()).is_some_and(|s| s.on)).collect();
    order.sort_by_key(|&i| -articles[i].published);
    let tokens: Vec<BTreeSet<String>> = articles.iter().map(|a| words(&a.title)).collect();
    // groups: (language, members, words of all members)
    let mut groups: Vec<(String, Vec<usize>, BTreeSet<String>)> = Vec::new();
    for i in order {
        let lang = by_id[articles[i].source.as_str()].language.clone();
        let best = groups.iter().enumerate()
            .filter(|(_, g)| g.0 == lang)
            .map(|(n, g)| (n, g.1.iter().map(|&m| similarity(&tokens[i], &tokens[m])).fold(0.0, f64::max)))
            .filter(|(_, s)| *s >= SAME_STORY)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((n, _)) => {
                groups[n].1.push(i);
                groups[n].2.extend(tokens[i].iter().cloned());
            }
            None => groups.push((lang, vec![i], tokens[i].clone())),
        }
    }
    let mut out: Vec<Story> = groups.into_iter().map(|(_, members, _)| {
        let mut seen = BTreeSet::new();
        let (mut left, mut center, mut right) = (0, 0, 0);
        for &m in &members {
            let src = by_id[articles[m].source.as_str()];
            if seen.insert(src.id.clone()) {
                match src.lean.side() {
                    Some(Side::Left) => left += 1,
                    Some(Side::Center) => center += 1,
                    Some(Side::Right) => right += 1,
                    None => {}
                }
            }
        }
        Story {
            title: articles[members[0]].title.clone(),
            newest: articles[members[0]].published,
            articles: members,
            left,
            center,
            right,
            sources: seen.len(),
        }
    }).collect();
    // most covered first, then newest
    out.sort_by(|a, b| b.sources.cmp(&a.sources).then(b.newest.cmp(&a.newest)));
    out
}

/// A stock the user follows.
#[derive(Clone, Debug, PartialEq)]
pub struct Ticker {
    pub symbol: String,
    /// company name words to spot in general news ("Apple"); may be empty
    pub name: String,
}

/// Does a headline mention this stock: the symbol as its own word (any case for 4+ letters, exact upper
/// case for short ones like "F"), or the company name.
pub fn mentions(title: &str, t: &Ticker) -> bool {
    let sym = t.symbol.trim();
    let word_hit = title.split(|c: char| !(c.is_alphanumeric() || c == '.')).any(|w| {
        if sym.len() >= 4 { w.eq_ignore_ascii_case(sym) } else { w == sym && w.chars().all(|c| c.is_uppercase() || c == '.') }
    });
    let name = t.name.trim().to_lowercase();
    word_hit || (name.len() >= 3 && title.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| w == name))
}

/// "Tue, 10 Jun 2025 14:03:00 GMT" | "+0200" (RSS) or "2025-06-10T14:03:00Z" | "+02:00" (Atom) -> unix seconds.
pub fn parse_date(text: &str) -> Option<i64> {
    let t = text.trim();
    if t.len() >= 19 && t.as_bytes().get(4) == Some(&b'-') {
        let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
        let (y, mo, d, h, mi, s) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
        let rest = &t[19..];
        let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        let offset = zone(rest)?;
        return Some(days(y, mo, d) * 86400 + h * 3600 + mi * 60 + s - offset);
    }
    // rfc 2822
    let parts: Vec<&str> = t.split_whitespace().collect();
    let p = if parts.first().is_some_and(|w| w.ends_with(',')) { &parts[1..] } else { &parts[..] };
    if p.len() < 4 {
        return None;
    }
    let d: i64 = p[0].parse().ok()?;
    let mo = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter().position(|m| p[1].to_lowercase().starts_with(m))? as i64 + 1;
    let mut y: i64 = p[2].parse().ok()?;
    if y < 100 { y += 2000; }
    let hms: Vec<i64> = p[3].split(':').filter_map(|x| x.parse().ok()).collect();
    let (h, mi, s) = (*hms.first()?, *hms.get(1).unwrap_or(&0), *hms.get(2).unwrap_or(&0));
    let offset = p.get(4).map(|z| zone(z)).unwrap_or(Some(0))?;
    Some(days(y, mo, d) * 86400 + h * 3600 + mi * 60 + s - offset)
}

fn zone(z: &str) -> Option<i64> {
    let z = z.trim();
    match z {
        "" | "Z" | "GMT" | "UT" | "UTC" => Some(0),
        "EST" => Some(-5 * 3600),
        "EDT" => Some(-4 * 3600),
        "CST" => Some(-6 * 3600),
        "CDT" => Some(-5 * 3600),
        "PST" => Some(-8 * 3600),
        "PDT" => Some(-7 * 3600),
        _ => {
            let sign = match z.as_bytes()[0] { b'+' => 1, b'-' => -1, _ => return Some(0) };
            let digits: String = z[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() < 4 { return Some(0); }
            Some(sign * (digits[..2].parse::<i64>().ok()? * 3600 + digits[2..4].parse::<i64>().ok()? * 60))
        }
    }
}

/// Days since 1970-01-01 (civil calendar).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// "3 h", "2 d" ago, for the list.
pub fn ago(now: i64, then: i64) -> (i64, &'static str) {
    let s = (now - then).max(0);
    if s < 3600 { (s / 60, "min") } else if s < 86400 { (s / 3600, "h") } else { (s / 86400, "d") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(id: &str, lean: Lean) -> Source {
        Source { id: id.into(), name: id.into(), feed: String::new(), lean, language: "en".into(), on: true }
    }
    fn art(source: &str, title: &str, at: i64) -> Article {
        Article { source: source.into(), title: title.into(), link: String::new(), published: at, summary: String::new() }
    }

    #[test]
    fn same_story_from_three_outlets_groups_and_counts_sides() {
        let sources = [src("a", Lean::Left), src("b", Lean::CenterLeft), src("c", Lean::Center), src("d", Lean::Right)];
        let arts = [
            art("a", "Central bank raises interest rates to fight inflation", 10),
            art("b", "Central bank raises rates again as inflation lingers", 9),
            art("c", "Interest rates: central bank raises by quarter point", 8),
            art("d", "Storm floods coastal towns overnight", 7),
        ];
        let s = stories(&arts, &sources);
        assert_eq!(s[0].articles.len(), 3);
        assert_eq!((s[0].left, s[0].center, s[0].right), (2, 1, 0));
        assert_eq!(s[0].blindspot(), Some(Side::Right));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn stocks_are_spotted_by_symbol_or_name() {
        let apple = Ticker { symbol: "AAPL".into(), name: "Apple".into() };
        let ford = Ticker { symbol: "F".into(), name: "Ford".into() };
        assert!(mentions("Apple shares climb after results", &apple));
        assert!(mentions("Why aapl fell today", &apple));
        assert!(!mentions("Pineapple prices soar", &apple));
        assert!(mentions("Ford recalls 10,000 trucks", &ford));
        assert!(!mentions("plan f for the summit", &Ticker { symbol: "F".into(), name: String::new() }));
    }

    #[test]
    fn feed_dates_both_formats() {
        assert_eq!(parse_date("Thu, 01 Jan 1970 00:01:00 GMT"), Some(60));
        assert_eq!(parse_date("Mon, 16 Nov 2026 10:00:00 +0100"), parse_date("2026-11-16T09:00:00Z"));
        assert_eq!(parse_date("2026-11-16T10:00:00.123+01:00"), parse_date("2026-11-16T09:00:00Z"));
        assert_eq!(parse_date("nonsense"), None);
    }
}
