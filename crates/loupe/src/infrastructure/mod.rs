//! Linux: processes, the virtual webcam (v4l2loopback), GNOME's remote desktop, UDP beacons.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::application::{LoupeError, Neighbours, System};
use crate::domain::{self, BEACON_PORT};

#[derive(Default)]
pub struct LinuxSystem {
    children: Mutex<HashMap<u32, Child>>,
}

fn run(program: &str, args: &[&str]) -> Result<(), LoupeError> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).output()
        .map_err(|e| LoupeError(format!("{program}: {e}")))?;
    if out.status.success() { Ok(()) } else {
        Err(LoupeError(format!("{program} failed: {}", String::from_utf8_lossy(&out.stderr).trim())))
    }
}

fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
    base.join("loupe")
}

impl System for LinuxSystem {
    fn start(&self, program: &str, args: &[String]) -> Result<u32, LoupeError> {
        let child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().map_err(|e| LoupeError(format!("{program}: {e}")))?;
        let id = child.id();
        self.children.lock().unwrap().insert(id, child);
        Ok(id)
    }

    fn stop(&self, handle: u32) {
        if let Some(mut child) = self.children.lock().unwrap().remove(&handle) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn running(&self, handle: u32) -> bool {
        let mut children = self.children.lock().unwrap();
        match children.get_mut(&handle).map(|c| c.try_wait()) {
            Some(Ok(None)) => true,
            Some(_) => { children.remove(&handle); false }
            None => false,
        }
    }

    fn camera(&self) -> Option<String> {
        // the first real camera: index 0 of its device (the others are metadata), never Loupe's own
        let mut found: Vec<(u32, String)> = std::fs::read_dir("/sys/class/video4linux").ok()?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let n: u32 = e.file_name().to_str()?.strip_prefix("video")?.parse().ok()?;
                let name = std::fs::read_to_string(e.path().join("name")).unwrap_or_default();
                let index = std::fs::read_to_string(e.path().join("index")).unwrap_or_else(|_| "0".into());
                (n != domain::CAMERA_DEVICE && !name.contains(domain::CAMERA_LABEL) && index.trim() == "0")
                    .then(|| (n, format!("/dev/video{n}")))
            })
            .collect();
        found.sort();
        found.into_iter().next().map(|(_, p)| p)
    }

    fn has(&self, program: &str) -> bool {
        std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(program).is_file()))
    }

    fn load_virtual_camera(&self, options: &[String]) -> Result<(), LoupeError> {
        let mut args = vec!["modprobe"];
        args.extend(options.iter().map(|s| s.as_str()));
        run("pkexec", &args)
    }

    fn virtual_camera_ready(&self) -> bool {
        Path::new(&domain::device_path()).exists()
    }

    fn share_screen(&self, code: Option<&str>) -> Result<(), LoupeError> {
        let Some(code) = code else {
            run("grdctl", &["rdp", "disable"])?;
            return run("grdctl", &["rdp", "clear-credentials"]);
        };
        let dir = data_dir();
        std::fs::create_dir_all(&dir).map_err(|e| LoupeError(e.to_string()))?;
        let (cert, key) = (dir.join("rdp-cert.pem"), dir.join("rdp-key.pem"));
        if !cert.exists() || !key.exists() {
            run("openssl", &["req", "-new", "-newkey", "rsa:3072", "-days", "3650", "-nodes", "-x509",
                "-subj", "/CN=loupe", "-keyout", key.to_str().unwrap_or_default(), "-out", cert.to_str().unwrap_or_default()])?;
        }
        run("grdctl", &["rdp", "set-tls-cert", cert.to_str().unwrap_or_default()])?;
        run("grdctl", &["rdp", "set-tls-key", key.to_str().unwrap_or_default()])?;
        run("grdctl", &["rdp", "set-credentials", domain::RDP_USER, code])?;
        run("gsettings", &["set", "org.gnome.desktop.remote-desktop.rdp", "screen-share-mode", "extend"])?;
        run("grdctl", &["rdp", "disable-view-only"])?;
        run("grdctl", &["rdp", "enable"])?;
        let _ = run("systemctl", &["--user", "enable", "--now", "gnome-remote-desktop.service"]);
        Ok(())
    }

    fn computer_name(&self) -> String {
        std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).ok().filter(|s| !s.is_empty())
            .unwrap_or_else(|| "computer".into())
    }

    fn random(&self) -> Vec<u8> {
        use std::io::Read;
        let mut bytes = vec![0u8; 16];
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut bytes);
        }
        bytes
    }
}

/// Beacons by UDP broadcast on the local network. Peers not heard for a while drop off.
pub struct Udp {
    socket: Option<UdpSocket>,
    seen: Mutex<HashMap<String, (String, Instant)>>,
}

impl Udp {
    pub fn new() -> Self {
        let socket = UdpSocket::bind(("0.0.0.0", BEACON_PORT)).ok().and_then(|s| {
            s.set_broadcast(true).ok()?;
            s.set_nonblocking(true).ok()?;
            Some(s)
        });
        Self { socket, seen: Mutex::new(HashMap::new()) }
    }
}

impl Default for Udp {
    fn default() -> Self { Self::new() }
}

impl Neighbours for Udp {
    fn announce(&self, beacon: &str) {
        if let Some(s) = &self.socket {
            let _ = s.send_to(beacon.as_bytes(), ("255.255.255.255", BEACON_PORT));
        }
    }

    fn heard(&self) -> Vec<(String, String)> {
        let mut seen = self.seen.lock().unwrap();
        if let Some(s) = &self.socket {
            let mut buf = [0u8; 512];
            while let Ok((n, from)) = s.recv_from(&mut buf) {
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                seen.insert(from.ip().to_string(), (text, Instant::now()));
            }
        }
        seen.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(8));
        seen.iter().map(|(addr, (text, _))| (text.clone(), addr.clone())).collect()
    }
}
