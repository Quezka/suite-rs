//! `--demo`: made-up headlines (no network, no files), for screenshots and trying it out.

use std::sync::Mutex;

use crate::application::{Feeds, Store};
use crate::domain::{Article, Source, Ticker};

pub const NOW: i64 = 1_794_830_400; // 2026-11-16 ~10:00 UTC

pub struct DemoFeeds;

const HEADLINES: &[(&str, &str, i64)] = &[
    ("bbc", "European Central Bank holds interest rates as inflation cools", 40),
    ("guardian", "ECB holds interest rates steady while inflation cools across eurozone", 55),
    ("wsj", "ECB keeps interest rates on hold, signals cuts as inflation cools", 70),
    ("fox", "ECB holds interest rates, inflation cools", 95),
    ("npr", "Heatwave pushes record wildfires across southern Europe", 120),
    ("guardian", "Record wildfires spread across southern Europe in heatwave", 150),
    ("aljazeera", "Wildfires across southern Europe force thousands to flee heatwave", 180),
    ("fox", "Border agents report record crossings at southern border this month", 200),
    ("nypost", "Record crossings at southern border reported by agents", 230),
    ("cnbc", "Apple shares climb after strong iPhone sales in China", 60),
    ("marketwatch", "Nvidia stock slips as chip export rules tighten", 90),
    ("ansa", "Manovra, via libera del Senato alla legge di bilancio", 30),
    ("repubblica", "Legge di bilancio, il Senato approva la manovra", 45),
    ("giornale", "Manovra approvata dal Senato, legge di bilancio verso la Camera", 75),
    ("ilpost", "Cosa c’è nella legge di bilancio approvata dal Senato", 100),
    ("fatto", "Sciopero dei treni venerdì, le fasce garantite", 130),
    ("ticker:AAPL", "Apple (AAPL) earnings preview: what to watch", 20),
    ("ticker:ENEL.MI", "Enel raises dividend guidance after strong quarter", 50),
];

impl Feeds for DemoFeeds {
    fn fetch(&self, _url: &str, source_id: &str) -> Result<Vec<Article>, String> {
        Ok(HEADLINES.iter().filter(|h| h.0 == source_id).enumerate().map(|(i, h)| Article {
            source: h.0.into(), title: h.1.into(), link: format!("https://example.org/{}/{i}", h.0),
            published: NOW - h.2 * 60, summary: String::new(),
        }).collect())
    }
    fn ticker_url(&self, symbol: &str) -> String {
        format!("demo:{symbol}")
    }
}

pub struct DemoStore(Mutex<(Vec<Source>, Vec<Article>, Vec<Ticker>)>);

impl DemoStore {
    pub fn new() -> Self {
        Self(Mutex::new((super::default_sources(), Vec::new(), vec![
            Ticker { symbol: "AAPL".into(), name: "Apple".into() },
            Ticker { symbol: "NVDA".into(), name: "Nvidia".into() },
            Ticker { symbol: "ENEL.MI".into(), name: "Enel".into() },
        ])))
    }
}

impl Default for DemoStore {
    fn default() -> Self { Self::new() }
}

impl Store for DemoStore {
    fn sources(&self) -> Vec<Source> { self.0.lock().unwrap().0.clone() }
    fn save_sources(&self, s: &[Source]) { self.0.lock().unwrap().0 = s.to_vec(); }
    fn articles(&self) -> Vec<Article> { self.0.lock().unwrap().1.clone() }
    fn save_articles(&self, a: &[Article]) { self.0.lock().unwrap().1 = a.to_vec(); }
    fn watchlist(&self) -> Vec<Ticker> { self.0.lock().unwrap().2.clone() }
    fn save_watchlist(&self, l: &[Ticker]) { self.0.lock().unwrap().2 = l.to_vec(); }
    fn now(&self) -> i64 { NOW }
}
