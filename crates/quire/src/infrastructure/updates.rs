//! updating quire from github releases (port of `infrastructure/updates.py`)
//! linux `.deb` @ `/opt/quire` -> `pkexec apt-get install` (system password dialog) -> restart
//! windows setup wizard copy -> new setup `/SILENT` closes quire, upgrades, starts again
//! else (from source, portable) -> update dialog links release page

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::application::errors::{AppError, AppResult};
use crate::application::ports::{ReleaseAsset, ReleaseFeed, ReleaseInfo, UpdateInstaller};

pub const API: &str = "https://api.github.com/repos/{repo}/releases/latest";
const CHUNK: usize = 256 * 1024;

/// one GET -> (status, body stream); err = no reach; trait => tests fake it
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<(u16, Box<dyn Read + Send>), String>;
}

pub struct UreqFetch(ureq::Agent);

impl Default for UreqFetch {
    fn default() -> Self {
        Self(ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(30))
            .timeout_read(std::time::Duration::from_secs(30)).build())
    }
}

impl Fetch for UreqFetch {
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<(u16, Box<dyn Read + Send>), String> {
        let mut req = self.0.get(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.call() {
            Ok(r) => Ok((r.status(), Box::new(r.into_reader()))),
            Err(ureq::Error::Status(code, r)) => Ok((code, Box::new(r.into_reader()))),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn err(text: &str) -> AppError {
    AppError::Update(text.into())
}

pub struct GitHubReleaseFeed {
    pub repo: String,
    pub user_agent: String,
    pub fetch: Box<dyn Fetch>,
    /// none -> `<tmp>/quire-update`
    pub folder: Option<PathBuf>,
}

impl GitHubReleaseFeed {
    pub fn new(repo: &str, version: &str) -> Self {
        Self { repo: repo.into(), user_agent: format!("Quire/{version}"), fetch: Box::new(UreqFetch::default()), folder: None }
    }
}

pub fn parse_release(data: &Value) -> Option<ReleaseInfo> {
    if data["draft"].as_bool() == Some(true) || data["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let assets = data["assets"].as_array().map(|list| list.iter().filter_map(|a| {
        let digest = a["digest"].as_str().unwrap_or("");
        Some(ReleaseAsset {
            name: a["name"].as_str()?.into(),
            url: a["browser_download_url"].as_str()?.into(),
            size: a["size"].as_u64()?,
            sha256: digest.strip_prefix("sha256:").map(str::to_string),
        })
    }).collect()).unwrap_or_default();
    Some(ReleaseInfo {
        version: data["tag_name"].as_str()?.trim_start_matches('v').into(),
        notes: data["body"].as_str().unwrap_or("").into(),
        page_url: data["html_url"].as_str().unwrap_or("").into(),
        assets,
    })
}

impl ReleaseFeed for GitHubReleaseFeed {
    fn latest(&self) -> AppResult<Option<ReleaseInfo>> {
        let headers = [("Accept", "application/vnd.github+json".to_string()), ("User-Agent", self.user_agent.clone())];
        let (status, mut body) = self.fetch.get(&API.replace("{repo}", &self.repo), &headers)
            .map_err(|_| err("Can't reach GitHub to check for updates. Check your internet connection."))?;
        match status {
            404 => return Ok(None), // no release yet | repo !public
            403 | 429 => return Err(err("GitHub is limiting update checks right now: try again in an hour.")),
            200..=299 => {}
            code => return Err(AppError::Update(format!("GitHub answered with an error ({code})."))),
        }
        let mut text = String::new();
        body.read_to_string(&mut text)
            .map_err(|_| err("Can't reach GitHub to check for updates. Check your internet connection."))?;
        let data: Value = serde_json::from_str(&text).map_err(|_| err("GitHub sent an answer Quire doesn't understand."))?;
        if data["draft"].as_bool() == Some(true) || data["prerelease"].as_bool() == Some(true) {
            return Ok(None);
        }
        parse_release(&data).map(Some).ok_or_else(|| err("GitHub sent an answer Quire doesn't understand."))
    }

    fn download(&self, asset: &ReleaseAsset, progress: &mut dyn FnMut(u64, u64)) -> AppResult<PathBuf> {
        let folder = self.folder.clone().unwrap_or_else(|| std::env::temp_dir().join("quire-update"));
        let unfinished = || err("The download didn't finish. Check your internet connection and try again.");
        std::fs::create_dir_all(&folder).map_err(|_| unfinished())?;
        let name = Path::new(&asset.name).file_name().map(|n| n.to_owned()).unwrap_or_else(|| "update".into());
        let target = folder.join(&name);
        let mut partial_name = name.clone();
        partial_name.push(".part");
        let partial = folder.join(partial_name);
        let fail = |e: AppError| {
            let _ = std::fs::remove_file(&partial);
            e
        };
        let (status, mut body) = self.fetch.get(&asset.url, &[("User-Agent", self.user_agent.clone())]).map_err(|_| unfinished())?;
        if !(200..300).contains(&status) {
            return Err(unfinished());
        }
        let mut out = std::fs::File::create(&partial).map_err(|_| unfinished())?;
        let mut digest = Sha256::new();
        let mut done: u64 = 0;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = body.read(&mut buf).map_err(|_| fail(unfinished()))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|_| fail(unfinished()))?;
            digest.update(&buf[..n]);
            done += n as u64;
            progress(done, asset.size);
        }
        drop(out);
        let hex: String = digest.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if done != asset.size || asset.sha256.as_ref().is_some_and(|s| s.to_lowercase() != hex) {
            return Err(fail(err("The download was damaged (its checksum doesn't match), so it wasn't installed. Try again.")));
        }
        std::fs::rename(&partial, &target).map_err(|_| fail(unfinished()))?;
        Ok(target)
    }
}

pub fn deb_architecture() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// `.deb` build @ `/opt/quire`
pub struct DebInstaller {
    pub executable: PathBuf,
}

impl DebInstaller {
    pub const PREFIX: &'static str = "/opt/quire";
}

impl UpdateInstaller for DebInstaller {
    fn supported(&self) -> bool {
        let exe = self.executable.canonicalize().unwrap_or_else(|_| self.executable.clone());
        exe.starts_with(Self::PREFIX) && on_path("pkexec")
    }
    fn pick(&self, assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
        let suffix = format!("_{}.deb", deb_architecture());
        assets.iter().find(|a| a.name.ends_with(&suffix)).cloned()
    }
    fn install(&self, path: &Path) -> AppResult<bool> {
        // pkexec = system password dialog; apt checks package + upgrades
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let out = Command::new("pkexec").args(["apt-get", "install", "-y", "--allow-downgrades"]).arg(path).output()
            .map_err(|_| err("Installing the update failed."))?;
        match out.status.code() {
            Some(0) => Ok(false), // installed -> restart
            Some(126) | Some(127) => Err(err("Installing was cancelled.")),
            _ => {
                let text = if out.stderr.is_empty() { &out.stdout } else { &out.stderr };
                let text = String::from_utf8_lossy(text);
                Err(match text.trim().lines().last() {
                    Some(line) => AppError::Update(format!("Installing the update failed: {line}")),
                    None => err("Installing the update failed."),
                })
            }
        }
    }
}

/// copy from setup wizard (uninstaller next to it)
pub struct WindowsSetupInstaller {
    pub executable: PathBuf,
}

impl UpdateInstaller for WindowsSetupInstaller {
    fn supported(&self) -> bool {
        self.executable.parent().and_then(|d| std::fs::read_dir(d).ok()).is_some_and(|mut entries| {
            entries.any(|e| e.ok().is_some_and(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.starts_with("unins") && n.ends_with(".exe")
            }))
        })
    }
    fn pick(&self, assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
        assets.iter().find(|a| a.name.ends_with("-windows-x64-setup.exe")).cloned()
    }
    fn install(&self, path: &Path) -> AppResult<bool> {
        let mut cmd = Command::new(path);
        cmd.args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        cmd.spawn().map_err(|_| err("Couldn't start the installer."))?;
        Ok(true) // setup takes over -> quit so it can replace files
    }
}

/// from source | portable -> download page
pub struct NoInstaller;

impl UpdateInstaller for NoInstaller {
    fn supported(&self) -> bool {
        false
    }
    fn pick(&self, _: &[ReleaseAsset]) -> Option<ReleaseAsset> {
        None
    }
    fn install(&self, _: &Path) -> AppResult<bool> {
        Err(err("This copy of Quire can't update itself."))
    }
}

pub fn platform_installer() -> Box<dyn UpdateInstaller> {
    let exe = std::env::current_exe().unwrap_or_default();
    if cfg!(windows) {
        Box::new(WindowsSetupInstaller { executable: exe })
    } else if cfg!(target_os = "linux") {
        Box::new(DebInstaller { executable: exe })
    } else {
        Box::new(NoInstaller)
    }
}
