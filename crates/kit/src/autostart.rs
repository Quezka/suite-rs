//! start app at login: linux = xdg autostart entry `~/.config/autostart/<app_id>.desktop`; windows = HKCU Run key
//! shared by * suite apps (was copied per app: quay, tether)

use std::path::PathBuf;
use std::process::Command;

const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// one app's login entry
#[derive(Clone, Debug)]
pub struct LoginItem {
    /// e.g. `io.github.quezka.Quire` -> file name + icon
    pub app_id: String,
    /// shown name + windows value name
    pub name: String,
    pub comment: String,
    /// full cmd: program + args (e.g. `--background`)
    pub command: Vec<String>,
    /// none -> `$XDG_CONFIG_HOME` | `~/.config`
    pub config_dir: Option<PathBuf>,
}

/// quote args for `.desktop` `Exec=` (desktop entry spec)
pub fn desktop_exec(args: &[String]) -> String {
    const SPECIAL: &str = " \t\n\"'\\><~|&;$*?#()`";
    args.iter().map(|a| {
        if !a.is_empty() && !a.chars().any(|c| SPECIAL.contains(c)) {
            return a.clone();
        }
        let mut out = String::from("\"");
        for c in a.chars() {
            if "\"`$\\".contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }).collect::<Vec<_>>().join(" ")
}

/// windows cmd line quoting (= python `subprocess.list2cmdline` for plain paths)
fn windows_line(args: &[String]) -> String {
    args.iter().map(|a| if a.is_empty() || a.contains([' ', '\t', '"']) { format!("\"{}\"", a.replace('"', "\\\"")) } else { a.clone() })
        .collect::<Vec<_>>().join(" ")
}

impl LoginItem {
    /// this exe + `args`
    pub fn for_this_exe(app_id: &str, name: &str, comment: &str, args: &[&str]) -> Self {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| name.to_lowercase());
        let mut command = vec![exe];
        command.extend(args.iter().map(|a| a.to_string()));
        Self { app_id: app_id.into(), name: name.into(), comment: comment.into(), command, config_dir: None }
    }

    pub fn supported(&self) -> bool {
        cfg!(any(windows, target_os = "linux"))
    }

    pub fn desktop_file(&self) -> PathBuf {
        let base = self.config_dir.clone()
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));
        base.join("autostart").join(format!("{}.desktop", self.app_id))
    }

    pub fn desktop_entry(&self) -> String {
        format!("[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}\nIcon={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            self.name, self.comment, desktop_exec(&self.command), self.app_id)
    }

    pub fn enabled(&self) -> bool {
        if cfg!(windows) && self.config_dir.is_none() {
            Command::new("reg").args(["query", RUN, "/v", &self.name]).output().is_ok_and(|o| o.status.success())
        } else {
            self.desktop_file().exists()
        }
    }

    /// err = system refused
    pub fn set_enabled(&self, on: bool) -> std::io::Result<()> {
        if cfg!(windows) && self.config_dir.is_none() {
            let status = if on {
                Command::new("reg").args(["add", RUN, "/v", &self.name, "/t", "REG_SZ", "/d", &windows_line(&self.command), "/f"]).output()?
            } else {
                Command::new("reg").args(["delete", RUN, "/v", &self.name, "/f"]).output()?
            };
            // delete missing value = fine
            return if status.status.success() || !on { Ok(()) } else { Err(std::io::Error::other("reg add failed")) };
        }
        let file = self.desktop_file();
        if on {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&file, self.desktop_entry())
        } else {
            match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_quoting_follows_the_spec() {
        let args: Vec<String> = ["/opt/quire/quire", "--background"].iter().map(|s| s.to_string()).collect();
        assert_eq!(desktop_exec(&args), "/opt/quire/quire --background");
        let spaced = vec!["/home/a b/q$x".to_string()];
        assert_eq!(desktop_exec(&spaced), "\"/home/a b/q\\$x\"");
    }

    #[test]
    fn entry_comes_and_goes() {
        let dir = std::env::temp_dir().join(format!("kit-autostart-{}", std::process::id()));
        let item = LoginItem { app_id: "io.github.quezka.Test".into(), name: "Test".into(), comment: "c".into(),
            command: vec!["test".into(), "--background".into()], config_dir: Some(dir.clone()) };
        assert!(!item.enabled());
        item.set_enabled(true).unwrap();
        assert!(item.enabled());
        assert!(std::fs::read_to_string(item.desktop_file()).unwrap().contains("Exec=test --background\nIcon=io.github.quezka.Test\n"));
        item.set_enabled(false).unwrap();
        item.set_enabled(false).unwrap();
        assert!(!item.enabled());
        let _ = std::fs::remove_dir_all(dir);
    }
}
