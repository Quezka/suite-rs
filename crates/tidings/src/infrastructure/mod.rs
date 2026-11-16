//! The web (curl: no TLS stack of our own, like Quay), RSS/Atom reading, files in ~/.local/share/tidings,
//! the starting list of sources.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::application::{Feeds, Store};
use crate::domain::{parse_date, Article, Lean, Source, Ticker};

pub struct WebFeeds;

impl Feeds for WebFeeds {
    fn fetch(&self, url: &str, source_id: &str) -> Result<Vec<Article>, String> {
        let out = Command::new("curl")
            .args(["-fsSL", "--max-time", "20", "--compressed", "-A", "Tidings/0.1 (news reader)", url])
            .stdin(Stdio::null()).stderr(Stdio::null()).output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("curl {}", out.status));
        }
        parse_feed(&String::from_utf8_lossy(&out.stdout), source_id)
    }

    fn ticker_url(&self, symbol: &str) -> String {
        format!("https://feeds.finance.yahoo.com/rss/2.0/headline?s={symbol}&region=US&lang=en-US")
    }
}

/// RSS 2.0 `<item>` or Atom `<entry>` -> articles. Tags in titles/summaries are stripped.
pub fn parse_feed(xml: &str, source_id: &str) -> Result<Vec<Article>, String> {
    let doc = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() })
        .map_err(|e| e.to_string())?;
    fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> { n.children().find(|c| c.tag_name().name() == name) }
    // a title can hold text + child tags (<b>…</b>): join every text piece, not just the first
    let text = |n: Option<roxmltree::Node>| n.map(|n| n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>().trim().to_string()).unwrap_or_default();
    let mut out = Vec::new();
    for n in doc.descendants().filter(|n| matches!(n.tag_name().name(), "item" | "entry")) {
        let title = clean(&text(child(n, "title")));
        let link = child(n, "link").map(|l| l.attribute("href").map(str::to_string).unwrap_or_else(|| l.text().unwrap_or_default().trim().to_string()))
            .filter(|l| !l.is_empty())
            .or_else(|| child(n, "guid").and_then(|g| g.text()).map(|t| t.trim().to_string()))
            .unwrap_or_default();
        if title.is_empty() || link.is_empty() {
            continue;
        }
        let date = ["pubDate", "published", "updated", "date"].iter().find_map(|t| child(n, t)).and_then(|d| d.text()).and_then(parse_date).unwrap_or(0);
        let summary = clean(&text(child(n, "description").or_else(|| child(n, "summary"))));
        out.push(Article { source: source_id.into(), title, link, published: date, summary: summary.chars().take(400).collect() });
    }
    Ok(out)
}

/// Strip HTML tags and the common entities, squeeze spaces.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
        .replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ").replace("&#8217;", "’").replace("&#8216;", "‘")
        .replace("&#8220;", "“").replace("&#8221;", "”").replace("&#8211;", "–");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Starting sources. Lean = common public ratings, rough; the user changes them on the Sources page.
pub fn default_sources() -> Vec<Source> {
    let s = |id: &str, name: &str, feed: &str, lean: Lean, lang: &str| Source {
        id: id.into(), name: name.into(), feed: feed.into(), lean, language: lang.into(), on: true,
    };
    vec![
        s("bbc", "BBC News", "https://feeds.bbci.co.uk/news/world/rss.xml", Lean::Center, "en"),
        s("guardian", "The Guardian", "https://www.theguardian.com/world/rss", Lean::CenterLeft, "en"),
        s("npr", "NPR", "https://feeds.npr.org/1001/rss.xml", Lean::CenterLeft, "en"),
        s("aljazeera", "Al Jazeera", "https://www.aljazeera.com/xml/rss/all.xml", Lean::CenterLeft, "en"),
        s("wsj", "Wall Street Journal", "https://feeds.a.dj.com/rss/RSSWorldNews.xml", Lean::Center, "en"),
        s("fox", "Fox News", "https://moxie.foxnews.com/google-publisher/world.xml", Lean::Right, "en"),
        s("nypost", "New York Post", "https://nypost.com/news/feed/", Lean::CenterRight, "en"),
        s("cnbc", "CNBC", "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=100003114", Lean::Center, "en"),
        s("marketwatch", "MarketWatch", "https://feeds.content.dowjones.io/public/rss/mw_topstories", Lean::Center, "en"),
        s("ansa", "ANSA", "https://www.ansa.it/sito/ansait_rss.xml", Lean::Center, "it"),
        s("ilpost", "Il Post", "https://www.ilpost.it/feed/", Lean::CenterLeft, "it"),
        s("repubblica", "la Repubblica", "https://www.repubblica.it/rss/homepage/rss2.0.xml", Lean::CenterLeft, "it"),
        s("corriere", "Corriere della Sera", "https://xml2.corriereobjects.it/rss/homepage.xml", Lean::Center, "it"),
        s("fatto", "Il Fatto Quotidiano", "https://www.ilfattoquotidiano.it/feed/", Lean::Left, "it"),
        s("giornale", "il Giornale", "https://www.ilgiornale.it/feed.xml", Lean::Right, "it"),
        s("libero", "Libero", "https://www.liberoquotidiano.it/rss.xml", Lean::Right, "it"),
    ]
}

pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
        let dir = base.join("tidings");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn read(&self, name: &str) -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(self.dir.join(name)).ok()?).ok()
    }

    fn write(&self, name: &str, v: Value) {
        let tmp = self.dir.join(format!("{name}.tmp"));
        if std::fs::write(&tmp, v.to_string()).is_ok() {
            let _ = std::fs::rename(tmp, self.dir.join(name));
        }
    }
}

impl Default for FileStore {
    fn default() -> Self { Self::new() }
}

impl Store for FileStore {
    fn sources(&self) -> Vec<Source> {
        let Some(Value::Array(list)) = self.read("sources.json") else { return default_sources() };
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        list.iter().map(|v| Source {
            id: s(v, "id"), name: s(v, "name"), feed: s(v, "feed"), lean: Lean::from_code(&s(v, "lean")),
            language: s(v, "language"), on: v.get("on").and_then(Value::as_bool).unwrap_or(true),
        }).filter(|s| !s.id.is_empty()).collect()
    }

    fn save_sources(&self, sources: &[Source]) {
        self.write("sources.json", Value::Array(sources.iter().map(|s| json!({
            "id": s.id, "name": s.name, "feed": s.feed, "lean": s.lean.code(), "language": s.language, "on": s.on,
        })).collect()));
    }

    fn articles(&self) -> Vec<Article> {
        let Some(Value::Array(list)) = self.read("articles.json") else { return Vec::new() };
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        list.iter().map(|v| Article {
            source: s(v, "source"), title: s(v, "title"), link: s(v, "link"),
            published: v.get("published").and_then(Value::as_i64).unwrap_or(0), summary: s(v, "summary"),
        }).collect()
    }

    fn save_articles(&self, articles: &[Article]) {
        self.write("articles.json", Value::Array(articles.iter().map(|a| json!({
            "source": a.source, "title": a.title, "link": a.link, "published": a.published, "summary": a.summary,
        })).collect()));
    }

    fn watchlist(&self) -> Vec<Ticker> {
        let Some(Value::Array(list)) = self.read("watchlist.json") else { return Vec::new() };
        list.iter().filter_map(|v| Some(Ticker {
            symbol: v.get("symbol")?.as_str()?.to_string(),
            name: v.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        })).collect()
    }

    fn save_watchlist(&self, list: &[Ticker]) {
        self.write("watchlist.json", Value::Array(list.iter().map(|t| json!({"symbol": t.symbol, "name": t.name})).collect()));
    }

    fn now(&self) -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    }
}

pub mod demo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_and_atom_both_read() {
        let rss = r#"<?xml version="1.0"?><rss><channel><item><title>Rates &amp; <b>inflation</b></title>
            <link>https://x/1</link><pubDate>Thu, 01 Jan 1970 00:01:00 GMT</pubDate><description>&lt;p&gt;Hi&lt;/p&gt;</description></item></channel></rss>"#;
        let a = parse_feed(rss, "s").unwrap();
        assert_eq!((a[0].title.as_str(), a[0].link.as_str(), a[0].published), ("Rates & inflation", "https://x/1", 60));
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>Storm</title><link href="https://y/2"/>
            <updated>1970-01-01T00:02:00Z</updated><summary>s</summary></entry></feed>"#;
        let b = parse_feed(atom, "s").unwrap();
        assert_eq!((b[0].link.as_str(), b[0].published), ("https://y/2", 120));
    }
}
