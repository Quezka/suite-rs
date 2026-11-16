//! quire @ login -> `--background` (straight to tray); entry = `quezka_kit::autostart`

use quezka_kit::autostart::LoginItem as KitLoginItem;

use crate::application::ports::LoginItem;
use crate::infrastructure::credentials::APP_ID;

pub const BACKGROUND_FLAG: &str = "--background";

pub struct SystemLoginItem(pub KitLoginItem);

impl SystemLoginItem {
    pub fn for_this_exe() -> Self {
        Self(KitLoginItem::for_this_exe(APP_ID, "Quire", "Start Quire in the background", &[BACKGROUND_FLAG]))
    }
}

impl LoginItem for SystemLoginItem {
    fn supported(&self) -> bool {
        self.0.supported()
    }
    fn enabled(&self) -> bool {
        self.0.enabled()
    }
    fn set_enabled(&self, on: bool) -> Result<(), String> {
        self.0.set_enabled(on).map_err(|e| e.to_string())
    }
}
