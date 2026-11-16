//! computer's own calendar (port of `infrastructure/system_calendar.py`)
//! linux: evolution data server `local` source follows file -> gnome calendar + evolution show it read-only
//! windows: no follow for local file -> hand file to calendar app => import

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::application::errors::{AppError, AppResult};
use crate::application::ports::SystemCalendar;

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local")).join("Quire")
    } else {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local").join("share")).join("Quire")
    }
}

fn config_dir() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

fn io(e: std::io::Error) -> AppError {
    AppError::Storage(format!("Couldn't write the calendar file: {e}"))
}

/// bytes: ics lines end `\r\n`; same bytes -> no write => watcher no reload
fn publish_file(file: &Path, text: &str) -> AppResult {
    if std::fs::read(file).is_ok_and(|old| old == text.as_bytes()) {
        return Ok(());
    }
    std::fs::create_dir_all(file.parent().unwrap_or(Path::new("."))).map_err(io)?;
    let partial = file.with_file_name("agenda.ics.part");
    std::fs::write(&partial, text).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    }
    std::fs::rename(&partial, file).map_err(io)
}

fn open_file(file: &Path) -> AppResult {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(file);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(file);
        c
    };
    cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        .map(|_| ()).map_err(|e| AppError::Storage(format!("Couldn't open the calendar: {e}")))
}

/// source file for evolution data server; picked up by itself
pub struct EvolutionCalendar {
    pub file: PathBuf,
    pub sources: PathBuf,
}

impl EvolutionCalendar {
    pub const SOURCE: &'static str = "quire-agenda.source";

    pub fn new(folder: Option<PathBuf>, sources: Option<PathBuf>) -> Self {
        Self {
            file: folder.unwrap_or_else(data_dir).join("agenda.ics"),
            sources: sources.unwrap_or_else(|| config_dir().join("evolution").join("sources")),
        }
    }
}

impl SystemCalendar for EvolutionCalendar {
    fn can_subscribe(&self) -> bool {
        true
    }
    fn publish(&self, text: &str) -> AppResult {
        publish_file(&self.file, text)
    }
    fn connect(&self) -> AppResult {
        std::fs::create_dir_all(&self.sources).map_err(io)?;
        let body = format!(
            "[Data Source]\nDisplayName=Quire\nEnabled=true\nParent=local-stub\n\n\
             [Calendar]\nBackendName=local\nColor=#5b5bd6\nSelected=true\n\n\
             [Local Backend]\nCustomFile=file://{}\nWritable=false\n",
            self.file.display()
        );
        std::fs::write(self.sources.join(Self::SOURCE), body).map_err(io)
    }
    fn disconnect(&self) -> AppResult {
        match std::fs::remove_file(self.sources.join(Self::SOURCE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }
    fn open(&self) -> AppResult {
        open_file(&self.file)
    }
}

pub struct WindowsCalendar {
    pub file: PathBuf,
}

impl SystemCalendar for WindowsCalendar {
    fn can_subscribe(&self) -> bool {
        false
    }
    fn publish(&self, text: &str) -> AppResult {
        publish_file(&self.file, text)
    }
    fn connect(&self) -> AppResult {
        Ok(())
    }
    fn disconnect(&self) -> AppResult {
        Ok(())
    }
    fn open(&self) -> AppResult {
        open_file(&self.file)
    }
}

pub fn platform_calendar() -> Box<dyn SystemCalendar> {
    if cfg!(windows) {
        Box::new(WindowsCalendar { file: data_dir().join("agenda.ics") })
    } else {
        Box::new(EvolutionCalendar::new(None, None))
    }
}
