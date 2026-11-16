//! Sends each call to the backend the config names (port of `infrastructure/cloud.py`).

use std::sync::Arc;

use super::firebase::FirebaseCloud;
use super::http::{Http, UreqHttp};
use super::quire_server::QuireServerCloud;
use crate::application::errors::AppResult;
use crate::application::ports::{CloudBackend, CloudConfig, CloudSession, SyncRecord, SERVER};

pub struct RoutingCloud {
    pub firebase: Box<dyn CloudBackend>,
    pub server: Box<dyn CloudBackend>,
}

impl RoutingCloud {
    /// The real thing: HTTPS through ureq.
    pub fn new(version: &str, device: &str) -> Self {
        let http: Arc<dyn Http> = Arc::new(UreqHttp::default());
        Self {
            firebase: Box::new(FirebaseCloud { http: http.clone() }),
            server: Box::new(QuireServerCloud { http, device: device.into(), version: version.into() }),
        }
    }

    fn pick(&self, c: &CloudConfig) -> &dyn CloudBackend {
        if c.backend == SERVER { &*self.server } else { &*self.firebase }
    }
}

impl CloudBackend for RoutingCloud {
    fn sign_up(&self, c: &CloudConfig, e: &str, p: &str, i: &str) -> AppResult<CloudSession> { self.pick(c).sign_up(c, e, p, i) }
    fn sign_in(&self, c: &CloudConfig, e: &str, p: &str) -> AppResult<CloudSession> { self.pick(c).sign_in(c, e, p) }
    fn refresh(&self, c: &CloudConfig, t: &str) -> AppResult<CloudSession> { self.pick(c).refresh(c, t) }
    fn pull(&self, c: &CloudConfig, s: &CloudSession, cur: Option<&str>) -> AppResult<(Vec<SyncRecord>, Option<String>)> { self.pick(c).pull(c, s, cur) }
    fn push(&self, c: &CloudConfig, s: &CloudSession, r: &[SyncRecord]) -> AppResult { self.pick(c).push(c, s, r) }
}
