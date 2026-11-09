//! Releases from GitHub, with a size and checksum check on every download. The HTTP goes through the system's
//! `curl` (on every Linux and in Windows 10 and later), so Quay carries no TLS stack of its own.
//! The API allows 60 anonymous requests an hour; a check of the whole suite takes one per app.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

use crate::application::ports::{Release, ReleaseAsset, ReleaseFeed};
use crate::application::QuayError;

pub struct GitHubFeed {
    agent: String,
    folder: PathBuf,
}

impl GitHubFeed {
    pub fn new(version: &str) -> Self {
        Self { agent: format!("Quay/{version}"), folder: std::env::temp_dir().join("quay-downloads") }
    }
}

fn curl() -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut c = Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up
    }
    c
}

/// Turns GitHub's "latest release" JSON into a Release; None for drafts and pre-releases.
pub fn parse_release(json: &str) -> Result<Option<Release>, QuayError> {
    let data: serde_json::Value =
        serde_json::from_str(json).map_err(|_| QuayError::new("GitHub sent an answer Quay doesn't understand."))?;
    if data["draft"].as_bool() == Some(true) || data["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let Some(tag) = data["tag_name"].as_str() else {
        return Err(QuayError::new("GitHub sent an answer Quay doesn't understand."));
    };
    let assets = data["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let digest = a["digest"].as_str().unwrap_or("");
                    Some(ReleaseAsset {
                        name: a["name"].as_str()?.to_string(),
                        url: a["browser_download_url"].as_str()?.to_string(),
                        size: a["size"].as_u64()?,
                        sha256: digest.strip_prefix("sha256:").map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        page_url: data["html_url"].as_str().unwrap_or("").to_string(),
        assets,
    }))
}

impl ReleaseFeed for GitHubFeed {
    fn latest(&self, repo: &str) -> Result<Option<Release>, QuayError> {
        let url = format!("https://api.github.com/repos/{repo}/releases/latest");
        let out = curl()
            .args(["-sS", "-L", "--max-time", "30", "-w", "\n%{http_code}", "-H", "Accept: application/vnd.github+json", "-A", &self.agent, &url])
            .output()
            .map_err(|_| QuayError::new("Can't check for updates: curl is missing."))?;
        if !out.status.success() {
            return Err(QuayError::new("Can't reach GitHub. Check your internet connection."));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let (body, code) = text.rsplit_once('\n').unwrap_or((&text, "0"));
        match code.trim() {
            "200" => parse_release(body),
            "404" => Ok(None), // no release yet, or the repository isn't public
            "403" | "429" => Err(QuayError::new("GitHub is limiting checks right now: try again in an hour.")),
            other => Err(QuayError::new(format!("GitHub answered with an error ({other})."))),
        }
    }

    fn download(&self, asset: &ReleaseAsset, progress: &dyn Fn(u64, u64)) -> Result<String, QuayError> {
        std::fs::create_dir_all(&self.folder).map_err(|_| QuayError::new("No room to download the installer."))?;
        let name = std::path::Path::new(&asset.name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let target = self.folder.join(&name);
        let partial = self.folder.join(format!("{name}.part"));
        let failed = || {
            let _ = std::fs::remove_file(&partial);
            QuayError::new("The download didn't finish. Check your internet connection and try again.")
        };
        // curl writes to its stdout; Quay reads it as it comes, to count and checksum it.
        let mut child = curl()
            .args(["-sS", "-L", "--fail", "--max-time", "1800", "-A", &self.agent, &asset.url])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| QuayError::new("Can't download: curl is missing."))?;
        let mut stdout = child.stdout.take().ok_or_else(failed)?;
        let mut file = std::fs::File::create(&partial).map_err(|_| failed())?;
        let mut digest = Sha256::new();
        let mut done = 0u64;
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            let n = stdout.read(&mut buffer).map_err(|_| failed())?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buffer[..n]).map_err(|_| failed())?;
            digest.update(&buffer[..n]);
            done += n as u64;
            progress(done, asset.size);
        }
        drop(file);
        if !child.wait().map_err(|_| failed())?.success() {
            return Err(failed());
        }
        let sum: String = digest.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if done != asset.size || asset.sha256.as_ref().is_some_and(|want| want.to_lowercase() != sum) {
            let _ = std::fs::remove_file(&partial);
            return Err(QuayError::new("The download was damaged (its checksum doesn't match), so it wasn't installed. Try again."));
        }
        std::fs::rename(&partial, &target).map_err(|_| failed())?;
        Ok(target.to_string_lossy().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_release_with_its_checksums() {
        let json = r#"{"tag_name":"v0.46.1","html_url":"https://x/r","assets":[
            {"name":"quire_0.46.1_amd64.deb","browser_download_url":"https://x/d","size":10,"digest":"sha256:ab"},
            {"name":"notes.txt","browser_download_url":"https://x/n","size":1}]}"#;
        let r = parse_release(json).unwrap().unwrap();
        assert_eq!(r.version, "0.46.1");
        assert_eq!(r.assets[0].sha256.as_deref(), Some("ab"));
        assert_eq!(r.assets[1].sha256, None);
        assert_eq!(parse_release(r#"{"tag_name":"v1","prerelease":true}"#).unwrap(), None);
        assert!(parse_release("nope").is_err());
    }
}
