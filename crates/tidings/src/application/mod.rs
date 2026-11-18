//! Use cases: refresh feeds, the story list, blindspots, market news, editing sources and the watchlist.

use crate::domain::{self, Article, Lean, Source, Story, Ticker};

/// The web: one source's latest articles (RSS or Atom).
pub trait Feeds: Send + Sync {
    fn fetch(&self, url: &str, source_id: &str) -> Result<Vec<Article>, String>;
    /// news about one stock (a per-symbol finance feed)
    fn ticker_url(&self, symbol: &str) -> String;
}

/// What's kept between runs.
pub trait Store: Send + Sync {
    fn sources(&self) -> Vec<Source>;
    fn save_sources(&self, sources: &[Source]);
    fn articles(&self) -> Vec<Article>;
    fn save_articles(&self, articles: &[Article]);
    fn watchlist(&self) -> Vec<Ticker>;
    fn save_watchlist(&self, list: &[Ticker]);
    fn now(&self) -> i64;
}

/// Articles older than this drop out <- 3 days of news, light on disk
pub const KEEP_SECONDS: i64 = 3 * 86400;

pub struct Report {
    pub fetched: usize,
    pub failed: Vec<String>,
}

pub struct News {
    feeds: std::sync::Arc<dyn Feeds>,
    store: Box<dyn Store>,
    pub articles: Vec<Article>,
    pub sources: Vec<Source>,
    pub watchlist: Vec<Ticker>,
}

impl News {
    pub fn new(feeds: std::sync::Arc<dyn Feeds>, store: Box<dyn Store>) -> Self {
        let (articles, sources, watchlist) = (store.articles(), store.sources(), store.watchlist());
        Self { feeds, store, articles, sources, watchlist }
    }

    /// What to fetch: (feed url, source id, name shown if it fails). Sources that are on + followed stocks.
    pub fn jobs(&self) -> Vec<(String, String, String)> {
        let mut out: Vec<_> = self.sources.iter().filter(|s| s.on).map(|s| (s.feed.clone(), s.id.clone(), s.name.clone())).collect();
        out.extend(self.watchlist.iter().map(|t| (self.feeds.ticker_url(&t.symbol), ticker_source(&t.symbol), t.symbol.clone())));
        out
    }

    /// The web side, shareable with a worker thread (fetching takes a while; the window stays live).
    pub fn feeds(&self) -> std::sync::Arc<dyn Feeds> {
        self.feeds.clone()
    }

    /// Run `jobs` with `feeds` anywhere (no `&self`): articles + names that failed.
    pub fn fetch_all(feeds: &dyn Feeds, jobs: &[(String, String, String)]) -> (Vec<Article>, Report) {
        let mut fresh = Vec::new();
        let mut failed = Vec::new();
        for (url, id, name) in jobs {
            match feeds.fetch(url, id) {
                Ok(a) => fresh.extend(a),
                Err(_) => failed.push(name.clone()),
            }
        }
        let fetched = fresh.len();
        (fresh, Report { fetched, failed })
    }

    /// Fetch everything here and merge (tests, command line).
    pub fn refresh(&mut self) -> Report {
        let (fresh, report) = Self::fetch_all(self.feeds.as_ref(), &self.jobs());
        self.merge(fresh);
        report
    }

    pub fn merge(&mut self, fresh: Vec<Article>) {
        let now = self.store.now();
        let mut by_link: std::collections::HashMap<String, Article> = self.articles.drain(..).map(|a| (a.link.clone(), a)).collect();
        for a in fresh {
            by_link.insert(a.link.clone(), a);
        }
        self.articles = by_link.into_values().filter(|a| a.published == 0 || now - a.published < KEEP_SECONDS).collect();
        self.articles.sort_by_key(|a| -a.published);
        self.store.save_articles(&self.articles);
    }

    /// Stories from the general news (not the per-stock feeds), optionally one language.
    pub fn stories(&self, language: Option<&str>) -> Vec<Story> {
        let sources: Vec<Source> = self.sources.iter().filter(|s| language.is_none_or(|l| s.language == l)).cloned().collect();
        domain::stories(&self.articles, &sources)
    }

    pub fn blindspots(&self, language: Option<&str>) -> Vec<Story> {
        self.stories(language).into_iter().filter(|s| s.blindspot().is_some()).collect()
    }

    /// Headlines about one stock: its own feed + general news that mentions it, newest first, no repeats.
    pub fn market_news(&self, t: &Ticker) -> Vec<&Article> {
        let own = ticker_source(&t.symbol);
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<&Article> = self.articles.iter()
            .filter(|a| a.source == own || domain::mentions(&a.title, t))
            .filter(|a| seen.insert(domain::words(&a.title)))
            .collect();
        out.sort_by_key(|a| -a.published);
        out
    }

    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn set_lean(&mut self, id: &str, lean: Lean) {
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == id) { s.lean = lean; }
        self.store.save_sources(&self.sources);
    }

    pub fn set_on(&mut self, id: &str, on: bool) {
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == id) { s.on = on; }
        self.store.save_sources(&self.sources);
    }

    /// A feed the user adds by hand (unrated until they say).
    pub fn add_source(&mut self, name: &str, feed: &str, language: &str) -> Result<(), &'static str> {
        let (name, feed) = (name.trim(), feed.trim());
        if name.is_empty() || !(feed.starts_with("http://") || feed.starts_with("https://")) {
            return Err("Give the source a name and a feed address starting with https://");
        }
        let id = format!("user-{}", self.sources.len());
        self.sources.push(Source { id, name: name.into(), feed: feed.into(), lean: Lean::Unrated, language: language.into(), on: true });
        self.store.save_sources(&self.sources);
        Ok(())
    }

    pub fn follow(&mut self, symbol: &str, name: &str) -> Result<(), &'static str> {
        let symbol = symbol.trim().to_uppercase();
        if symbol.is_empty() || symbol.len() > 12 || !symbol.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '^') {
            return Err("A stock symbol is a few letters, like AAPL or ENEL.MI");
        }
        if !self.watchlist.iter().any(|t| t.symbol == symbol) {
            self.watchlist.push(Ticker { symbol, name: name.trim().into() });
            self.store.save_watchlist(&self.watchlist);
        }
        Ok(())
    }

    pub fn unfollow(&mut self, symbol: &str) {
        self.watchlist.retain(|t| t.symbol != symbol);
        self.store.save_watchlist(&self.watchlist);
    }

    pub fn now(&self) -> i64 {
        self.store.now()
    }

    /// Newest article's time (0 = none): a cheap "how fresh are we" for auto-refresh.
    pub fn newest(&self) -> i64 {
        self.articles.iter().map(|a| a.published).max().unwrap_or(0)
    }
}

/// Articles from a stock's own feed carry this as their source.
pub fn ticker_source(symbol: &str) -> String {
    format!("ticker:{symbol}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeFeeds;
    impl Feeds for FakeFeeds {
        fn fetch(&self, url: &str, id: &str) -> Result<Vec<Article>, String> {
            if url == "bad" { return Err("down".into()); }
            Ok(vec![Article { source: id.into(), title: format!("Apple news from {id}"), link: format!("{url}/1"), published: 990, summary: String::new() }])
        }
        fn ticker_url(&self, s: &str) -> String { format!("ticker/{s}") }
    }
    #[derive(Default)]
    struct MemStore(Mutex<(Vec<Article>, Vec<Ticker>)>);
    impl Store for MemStore {
        fn sources(&self) -> Vec<Source> {
            vec![Source { id: "a".into(), name: "A".into(), feed: "ok".into(), lean: Lean::Center, language: "en".into(), on: true },
                 Source { id: "b".into(), name: "B".into(), feed: "bad".into(), lean: Lean::Left, language: "en".into(), on: true }]
        }
        fn save_sources(&self, _: &[Source]) {}
        fn articles(&self) -> Vec<Article> { self.0.lock().unwrap().0.clone() }
        fn save_articles(&self, a: &[Article]) { self.0.lock().unwrap().0 = a.to_vec(); }
        fn watchlist(&self) -> Vec<Ticker> { self.0.lock().unwrap().1.clone() }
        fn save_watchlist(&self, l: &[Ticker]) { self.0.lock().unwrap().1 = l.to_vec(); }
        fn now(&self) -> i64 { 1000 }
    }

    #[test]
    fn refresh_merges_reports_failures_and_finds_stock_news() {
        let mut n = News::new(std::sync::Arc::new(FakeFeeds), Box::new(MemStore::default()));
        n.follow("aapl", "Apple").unwrap();
        let r = n.refresh();
        assert_eq!(r.failed, vec!["B".to_string()]);
        assert_eq!(n.articles.len(), 2);
        let apple = n.watchlist[0].clone();
        assert_eq!(n.market_news(&apple).len(), 2);
        assert!(n.follow("not a symbol!", "").is_err());
    }

    #[test]
    fn old_articles_drop_out() {
        let mut n = News::new(std::sync::Arc::new(FakeFeeds), Box::new(MemStore::default()));
        n.merge(vec![Article { source: "a".into(), title: "x".into(), link: "old".into(), published: 1000 - KEEP_SECONDS - 1, summary: String::new() }]);
        assert!(n.articles.is_empty());
    }
}
