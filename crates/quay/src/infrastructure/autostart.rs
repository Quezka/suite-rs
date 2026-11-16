//! quay @ login; entry rules @ `quezka_kit::autostart` (desktop entry | Run key)

use quezka_kit::autostart::LoginItem;

use crate::application::ports::Autostart;
use crate::application::QuayError;

pub struct KitAutostart(pub LoginItem);

impl KitAutostart {
    pub fn new(config: Option<std::path::PathBuf>) -> Self {
        // linux: plain `quay` (deb on PATH) = same file as before; windows: full exe path
        let program = if cfg!(windows) {
            std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "quay".into())
        } else {
            "quay".into()
        };
        Self(LoginItem {
            app_id: "io.github.quezka.Quay".into(), name: "Quay".into(), comment: "Checks for updates to the Quezka suite".into(),
            command: vec![program, "--autostart".into()], config_dir: config,
        })
    }
}

impl Autostart for KitAutostart {
    fn enabled(&self) -> bool {
        self.0.enabled()
    }
    fn set(&self, enabled: bool) -> Result<(), QuayError> {
        self.0.set_enabled(enabled).map_err(|_| QuayError::new("Couldn't change starting with the computer."))
    }
}

pub fn for_this_computer() -> Option<std::sync::Arc<dyn Autostart>> {
    if cfg!(any(windows, target_os = "linux")) {
        Some(std::sync::Arc::new(KitAutostart::new(None)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_entry_comes_and_goes() {
        let dir = std::env::temp_dir().join(format!("quay-autostart-{}", std::process::id()));
        let a = KitAutostart::new(Some(dir.clone()));
        assert!(!a.enabled());
        a.set(true).unwrap();
        assert!(a.enabled());
        assert!(std::fs::read_to_string(a.0.desktop_file()).unwrap().contains("Exec=quay --autostart\n"));
        a.set(false).unwrap();
        a.set(false).unwrap();
        assert!(!a.enabled());
        let _ = std::fs::remove_dir_all(dir);
    }
}
