//! how quire runs (port of `services/startup.py`): bg on window close (reminders + sync go on) + start at login

use std::rc::Rc;

use super::errors::{AppError, AppResult};
use super::ports::{KeyValueStore, LoginItem};

const BACKGROUND: &str = "startup.background";

pub struct StartupService {
    pub login_item: Rc<dyn LoginItem>,
    pub settings: Rc<dyn KeyValueStore>,
}

impl StartupService {
    pub fn background_on_close(&self) -> bool {
        self.settings.get(BACKGROUND).as_deref() != Some("0")
    }

    pub fn set_background_on_close(&self, on: bool) -> AppResult {
        self.settings.set(BACKGROUND, Some(if on { "1" } else { "0" }))
    }

    pub fn can_start_on_login(&self) -> bool {
        self.login_item.supported()
    }

    pub fn start_on_login(&self) -> bool {
        self.login_item.supported() && self.login_item.enabled()
    }

    pub fn set_start_on_login(&self, on: bool) -> AppResult {
        self.login_item.set_enabled(on)
            .map_err(|_| AppError::Startup("Couldn't change whether Quire starts when you log in.".into()))
    }

    /// rewrite entry if on -> follows quire if moved | updated; err ignored <- never block start
    pub fn refresh_login_item(&self) {
        if self.start_on_login() {
            let _ = self.login_item.set_enabled(true);
        }
    }
}
