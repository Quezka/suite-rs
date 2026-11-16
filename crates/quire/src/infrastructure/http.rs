//! HTTP for the cloud backends: a small trait (so tests use a fake) and ureq with rustls (no system TLS needed).

use std::time::Duration;

/// One request: (status, body). Err only when the server couldn't be reached.
pub trait Http: Send + Sync {
    fn send(&self, method: &str, url: &str, headers: &[(&str, String)], body: Option<String>) -> Result<(u16, String), String>;
}

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self { agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build() }
    }
}

impl Http for UreqHttp {
    fn send(&self, method: &str, url: &str, headers: &[(&str, String)], body: Option<String>) -> Result<(u16, String), String> {
        let mut req = self.agent.request(method, url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let result = match body {
            Some(b) => req.send_string(&b),
            None => req.call(),
        };
        match result {
            Ok(r) => Ok((r.status(), r.into_string().unwrap_or_default())),
            Err(ureq::Error::Status(code, r)) => Ok((code, r.into_string().unwrap_or_default())),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
