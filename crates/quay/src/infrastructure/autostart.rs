//! Starting Quay at login: a desktop entry on Linux, the Run key on Windows.

use std::path::PathBuf;
use std::process::Command;

use crate::application::ports::Autostart;
use crate::application::QuayError;

const LINUX_ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Quay\nComment=Checks for updates to the Quezka suite\n\
Exec=quay --autostart\nIcon=io.github.quezka.Quay\nTerminal=false\nX-GNOME-Autostart-enabled=true\n";

pub struct LinuxAutostart {
    file: PathBuf,
}

impl LinuxAutostart {
    pub fn new(config: Option<PathBuf>) -> Self {
        let base = config
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));
        Self { file: base.join("autostart").join("io.github.quezka.Quay.desktop") }
    }
}

impl Autostart for LinuxAutostart {
    fn enabled(&self) -> bool {
        self.file.exists()
    }
    fn set(&self, enabled: bool) -> Result<(), QuayError> {
        let failed = |_| QuayError::new("Couldn't change starting with the computer.");
        if enabled {
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir).map_err(failed)?;
            }
            std::fs::write(&self.file, LINUX_ENTRY).map_err(failed)
        } else {
            match std::fs::remove_file(&self.file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(e)),
                _ => Ok(()),
            }
        }
    }
}

pub struct WindowsAutostart;

const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

impl Autostart for WindowsAutostart {
    fn enabled(&self) -> bool {
        Command::new("reg").args(["query", RUN, "/v", "Quay"]).output().is_ok_and(|o| o.status.success())
    }
    fn set(&self, enabled: bool) -> Result<(), QuayError> {
        let exe = std::env::current_exe().map_err(|_| QuayError::new("Couldn't find Quay itself."))?;
        let value = format!("\"{}\" --autostart", exe.display());
        let ok = if enabled {
            Command::new("reg").args(["add", RUN, "/v", "Quay", "/t", "REG_SZ", "/d", &value, "/f"]).status()
        } else {
            Command::new("reg").args(["delete", RUN, "/v", "Quay", "/f"]).status()
        };
        if ok.is_ok_and(|s| s.success()) || !enabled {
            Ok(())
        } else {
            Err(QuayError::new("Couldn't change starting with the computer."))
        }
    }
}

pub fn for_this_computer() -> Option<std::sync::Arc<dyn Autostart>> {
    if cfg!(windows) {
        Some(std::sync::Arc::new(WindowsAutostart))
    } else if cfg!(target_os = "linux") {
        Some(std::sync::Arc::new(LinuxAutostart::new(None)))
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
        let a = LinuxAutostart::new(Some(dir.clone()));
        assert!(!a.enabled());
        a.set(true).unwrap();
        assert!(a.enabled());
        a.set(false).unwrap();
        a.set(false).unwrap();
        assert!(!a.enabled());
        let _ = std::fs::remove_dir_all(dir);
    }
}
