//! do not disturb switch for desktop (= python `infrastructure/quiet.py`)

use std::cell::RefCell;
use std::process::Command;

use crate::application::quiet::QuietSwitch;

const SCHEMA: &str = "org.gnome.desktop.notifications";

/// no way to silence desktop (windows, kde, ... for now)
pub struct NoQuiet;

impl QuietSwitch for NoQuiet {
    fn name(&self) -> String {
        String::new()
    }
    fn available(&self) -> bool {
        false
    }
    fn set_quiet(&self, _: bool) -> Result<(), String> {
        Ok(())
    }
}

/// gnome: banners + sounds off; remembers old val -> back as was
#[derive(Default)]
pub struct GnomeQuiet {
    before: RefCell<Option<String>>,
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl QuietSwitch for GnomeQuiet {
    fn name(&self) -> String {
        "GNOME".into()
    }
    fn available(&self) -> bool {
        std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("gsettings").is_file()))
    }
    fn set_quiet(&self, on: bool) -> Result<(), String> {
        if on {
            // asked twice -> 1st answer = way things were
            if self.before.borrow().is_none() {
                let was = gsettings(&["get", SCHEMA, "show-banners"])?;
                *self.before.borrow_mut() = Some(if was.is_empty() { "true".into() } else { was });
            }
            gsettings(&["set", SCHEMA, "show-banners", "false"]).map(|_| ())
        } else {
            let wanted = self.before.borrow().clone().unwrap_or_else(|| "true".into());
            gsettings(&["set", SCHEMA, "show-banners", &wanted])?;
            let now = gsettings(&["get", SCHEMA, "show-banners"])?;
            if !now.is_empty() && now != wanted {
                return Err(format!("show-banners is still {now}"));
            }
            *self.before.borrow_mut() = None;
            Ok(())
        }
    }
}

pub fn platform_quiet() -> Box<dyn QuietSwitch> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    if cfg!(unix) && (desktop.contains("gnome") || desktop.contains("ubuntu")) {
        Box::new(GnomeQuiet::default())
    } else {
        Box::new(NoQuiet)
    }
}
