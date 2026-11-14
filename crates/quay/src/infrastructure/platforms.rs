//! Installing and starting apps on Linux (.deb through pkexec apt) and Windows (the apps' Inno setup).

use std::process::{Command, Stdio};

use crate::application::ports::{Platform, ReleaseAsset};
use crate::application::QuayError;
use crate::domain::catalog::SuiteApp;

pub fn deb_architecture() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// Where a command is on PATH.
fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(name)).find(|p| p.is_file())
}

pub struct DebPlatform;

impl Platform for DebPlatform {
    fn installed_version(&self, app: &SuiteApp) -> Option<String> {
        let out = Command::new("dpkg-query").args(["-W", "-f=${Status}|${Version}", app.package]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let (status, version) = text.split_once('|')?;
        if !status.ends_with("installed") || status.contains("not-installed") {
            return None;
        }
        Some(version.trim().to_string()).filter(|v| !v.is_empty())
    }

    fn pick<'a>(&self, assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset> {
        let arch = format!("_{}.deb", deb_architecture());
        // Natter's .deb is the same on every machine.
        assets.iter().find(|a| a.name.ends_with(&arch) || a.name.ends_with("_all.deb"))
    }

    fn install(&self, _app: &SuiteApp, path: &str) -> Result<(), QuayError> {
        let full = std::fs::canonicalize(path).map_err(|_| QuayError::new("The downloaded installer is missing."))?;
        // pkexec shows the system's password dialog; apt checks the package and upgrades it.
        let out = Command::new("pkexec")
            .args(["apt-get", "install", "-y", "--allow-downgrades"])
            .arg(full)
            .output()
            .map_err(|_| QuayError::new("Can't ask for permission to install (pkexec is missing)."))?;
        match out.status.code() {
            Some(0) => Ok(()),
            Some(126) | Some(127) => Err(QuayError::new("Installing was cancelled.")),
            _ => {
                let text = String::from_utf8_lossy(if out.stderr.is_empty() { &out.stdout } else { &out.stderr }).to_string();
                let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                Err(QuayError::new(if last.is_empty() { "Installing failed.".to_string() } else { format!("Installing failed: {last}") }))
            }
        }
    }

    fn launch(&self, app: &SuiteApp) -> Result<(), QuayError> {
        let command = which(app.command).ok_or_else(|| QuayError::new("Can't find the program to start."))?;
        Command::new(command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|_| QuayError::new("Couldn't start the program."))
    }
}

/// Windows lists what Inno Setup installed under `{AppId}_is1`, for this user or for everyone. Read with `reg`,
/// which every Windows has.
pub struct WindowsPlatform;

#[cfg_attr(not(windows), allow(unused_mut))]
fn hidden(mut c: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

/// A value of an Inno Setup entry, from the output of `reg query <key> /v <name>`.
pub fn reg_value(output: &str, name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next()? == name).then(|| {
            let _kind = parts.next();
            parts.collect::<Vec<_>>().join(" ")
        })
    }).filter(|v| !v.is_empty())
}

impl WindowsPlatform {
    fn read(&self, app: &SuiteApp, name: &str) -> Option<String> {
        for hive in ["HKCU", "HKLM"] {
            let key = format!(r"{hive}\Software\Microsoft\Windows\CurrentVersion\Uninstall\{{{}}}_is1", app.windows_guid);
            let mut c = hidden(Command::new("reg"));
            if let Ok(out) = c.args(["query", &key, "/v", name]).output() {
                if out.status.success() {
                    if let Some(v) = reg_value(&String::from_utf8_lossy(&out.stdout), name) {
                        return Some(v);
                    }
                }
            }
        }
        None
    }
}

impl Platform for WindowsPlatform {
    fn installed_version(&self, app: &SuiteApp) -> Option<String> {
        self.read(app, "DisplayVersion")
    }

    fn pick<'a>(&self, assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset> {
        assets.iter().find(|a| a.name.ends_with("-windows-x64-setup.exe"))
    }

    fn install(&self, _app: &SuiteApp, path: &str) -> Result<(), QuayError> {
        // The setup closes the app, upgrades it in place and starts it again.
        Command::new(path)
            .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"])
            .spawn()
            .map(|_| ())
            .map_err(|_| QuayError::new("Couldn't start the installer."))
    }

    fn launch(&self, app: &SuiteApp) -> Result<(), QuayError> {
        let folder = self.read(app, "InstallLocation").ok_or_else(|| QuayError::new("Can't find the program to start."))?;
        let exe = std::path::Path::new(&folder).join(app.windows_exe);
        if !exe.exists() {
            return Err(QuayError::new("Can't find the program to start."));
        }
        Command::new(exe).spawn().map(|_| ()).map_err(|_| QuayError::new("Couldn't start the program."))
    }
}

/// macOS and the rest: nothing is installed by Quay.
pub struct NoPlatform;

impl Platform for NoPlatform {
    fn installed_version(&self, _app: &SuiteApp) -> Option<String> {
        None
    }
    fn pick<'a>(&self, _assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset> {
        None
    }
    fn install(&self, _app: &SuiteApp, _path: &str) -> Result<(), QuayError> {
        Err(QuayError::new("This computer isn't supported by Quay yet."))
    }
    fn launch(&self, _app: &SuiteApp) -> Result<(), QuayError> {
        Err(QuayError::new("This computer isn't supported by Quay yet."))
    }
}

pub fn for_this_computer() -> std::sync::Arc<dyn Platform> {
    if cfg!(windows) {
        std::sync::Arc::new(WindowsPlatform)
    } else if cfg!(target_os = "linux") {
        std::sync::Arc::new(DebPlatform)
    } else {
        std::sync::Arc::new(NoPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset { name: name.into(), url: String::new(), size: 1, sha256: None }
    }

    #[test]
    fn picks_the_right_installer() {
        let assets = [asset("x.tar.gz"), asset("Quire-1-windows-x64-setup.exe"), asset(&format!("quire_1_{}.deb", deb_architecture()))];
        assert!(DebPlatform.pick(&assets).unwrap().name.ends_with(".deb"));
        assert!(WindowsPlatform.pick(&assets).unwrap().name.ends_with(".exe"));
        assert!(DebPlatform.pick(&[asset("natter_0.9.2_all.deb")]).is_some());
    }

    #[test]
    fn reads_reg_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\...\\{X}_is1\r\n    DisplayVersion    REG_SZ    0.46.1\r\n";
        assert_eq!(reg_value(out, "DisplayVersion").as_deref(), Some("0.46.1"));
        let path = "    InstallLocation    REG_SZ    C:\\Program Files\\Quire\\\r\n";
        assert_eq!(reg_value(path, "InstallLocation").as_deref(), Some("C:\\Program Files\\Quire\\"));
    }
}
