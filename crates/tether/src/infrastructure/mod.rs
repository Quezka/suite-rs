//! The link (TCP + AES-256-GCM), the beacon, pairing storage, and the desktop side (notifications, clipboard,
//! files, picking a file).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};

use crate::application::{Desktop, LinkEvent, PhoneLink};
use crate::domain::{self, BEACON_PORT, FILE_PIECE, LINK_PORT};

pub fn random(n: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    bytes
}

fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
    base.join("tether")
}

pub fn computer_name() -> String {
    std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "computer".into())
}

/// Paired phones: id → (name, long-term key). Kept in ~/.local/share/tether/paired.json (only this user can read it).
#[derive(Default)]
pub struct Pairings {
    pub phones: HashMap<String, (String, Vec<u8>)>,
    pub code: Option<String>,
    pub computer_id: String,
}

impl Pairings {
    pub fn load() -> Self {
        let path = data_dir().join("paired.json");
        let v: Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
        let mut phones = HashMap::new();
        if let Some(map) = v.get("phones").and_then(Value::as_object) {
            for (id, p) in map {
                if let (Some(name), Some(key)) = (p.get("name").and_then(Value::as_str), p.get("key").and_then(Value::as_str)) {
                    if let Ok(k) = B64.decode(key) {
                        phones.insert(id.clone(), (name.to_string(), k));
                    }
                }
            }
        }
        let computer_id = v.get("id").and_then(Value::as_str).map(str::to_string)
            .unwrap_or_else(|| random(8).iter().map(|b| format!("{b:02x}")).collect());
        let p = Self { phones, code: None, computer_id };
        p.save();
        p
    }

    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let phones: serde_json::Map<String, Value> = self.phones.iter()
            .map(|(id, (name, key))| (id.clone(), json!({"name": name, "key": B64.encode(key)}))).collect();
        let path = dir.join("paired.json");
        let _ = std::fs::write(&path, json!({"id": self.computer_id, "phones": phones}).to_string());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
}


struct Crypto(Aes256Gcm);

impl Crypto {
    fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let iv = random(12);
        let mut out = iv.clone();
        out.extend(self.0.encrypt(Nonce::from_slice(&iv), plain).unwrap_or_default());
        out
    }
    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 12 + 16 {
            return None;
        }
        self.0.decrypt(Nonce::from_slice(&sealed[..12]), &sealed[12..]).ok()
    }
}

fn read_frame(s: &mut TcpStream) -> Option<Vec<u8>> {
    let mut header = [0u8; 4];
    s.read_exact(&mut header).ok()?;
    let n = domain::frame_len(header)?;
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// The computer's side of the link: listens for the phone, shakes hands, then passes messages both ways.
pub struct Link {
    pub pairings: Arc<Mutex<Pairings>>,
    outgoing: Arc<Mutex<Option<Sender<Value>>>>,
}

impl Link {
    pub fn start(events: Sender<LinkEvent>) -> Self {
        let pairings = Arc::new(Mutex::new(Pairings::load()));
        let outgoing: Arc<Mutex<Option<Sender<Value>>>> = Arc::new(Mutex::new(None));
        // beacon: say where we are every two seconds
        let id = pairings.lock().unwrap().computer_id.clone();
        std::thread::spawn(move || {
            let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return };
            let _ = sock.set_broadcast(true);
            let text = domain::beacon(&computer_name(), &id);
            loop {
                let _ = sock.send_to(text.as_bytes(), ("255.255.255.255", BEACON_PORT));
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        let (p, o) = (pairings.clone(), outgoing.clone());
        std::thread::spawn(move || {
            let Ok(listener) = TcpListener::bind(("0.0.0.0", LINK_PORT)) else { return };
            for stream in listener.incoming().flatten() {
                let (p, o, ev) = (p.clone(), o.clone(), events.clone());
                std::thread::spawn(move || serve(stream, p, o, ev));
            }
        });
        Self { pairings, outgoing }
    }

}

impl PhoneLink for Link {
    fn send(&self, message: Value) -> bool {
        self.outgoing.lock().unwrap().as_ref().is_some_and(|tx| tx.send(message).is_ok())
    }

    fn paired(&self) -> Vec<(String, String)> {
        self.pairings.lock().unwrap().phones.iter().map(|(id, (n, _))| (id.clone(), n.clone())).collect()
    }

    /// A fresh pairing code (shown on screen, typed on the phone).
    fn new_code(&self) -> String {
        let code = domain::pairing_code(&random(8));
        self.pairings.lock().unwrap().code = Some(code.clone());
        code
    }

    fn forget(&self, id: &str) {
        let mut p = self.pairings.lock().unwrap();
        p.phones.remove(id);
        p.save();
    }

    /// Send a file in pieces.
    fn send_file(&self, path: &std::path::Path) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let id: String = random(6).iter().map(|b| format!("{b:02x}")).collect();
        self.send(json!({"t": "file_start", "id": id, "name": name, "size": data.len()}));
        for piece in data.chunks(FILE_PIECE) {
            self.send(json!({"t": "file_piece", "id": id, "data": B64.encode(piece)}));
        }
        if self.send(json!({"t": "file_end", "id": id})) { Ok(()) } else { Err("The phone isn't connected.".into()) }
    }
}

fn serve(mut stream: TcpStream, pairings: Arc<Mutex<Pairings>>, outgoing: Arc<Mutex<Option<Sender<Value>>>>, events: Sender<LinkEvent>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(hello) = read_frame(&mut stream).and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { return };
    let get = |k: &str| hello.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let (phone_id, phone_name) = (get("id"), get("name"));
    let Ok(phone_nonce) = B64.decode(get("nonce")) else { return };
    let pairing = hello.get("pairing").and_then(Value::as_bool).unwrap_or(false);
    let secret: Vec<u8> = {
        let p = pairings.lock().unwrap();
        if pairing {
            match &p.code { Some(c) => c.as_bytes().to_vec(), None => return }
        } else {
            match p.phones.get(&phone_id) { Some((_, k)) => k.clone(), None => return }
        }
    };
    let desktop_nonce = random(16);
    let reply = json!({"t": "hello", "name": computer_name(), "nonce": B64.encode(&desktop_nonce)}).to_string();
    if stream.write_all(&domain::frame(reply.as_bytes())).is_err() {
        return;
    }
    let key = domain::session_key(&secret, &phone_nonce, &desktop_nonce);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else { return };
    let crypto = Arc::new(Crypto(cipher));
    // the phone proves it has the key
    let Some(auth) = read_frame(&mut stream).and_then(|b| crypto.open(&b)) else { return };
    if serde_json::from_slice::<Value>(&auth).ok().and_then(|v| v.get("t").cloned()) != Some(json!("auth")) {
        return;
    }
    let (tx, rx): (Sender<Value>, Receiver<Value>) = channel();
    if pairing {
        let long_term = random(32);
        let msg = json!({"t": "paired", "key": B64.encode(&long_term), "id": pairings.lock().unwrap().computer_id});
        if stream.write_all(&domain::frame(&crypto.seal(msg.to_string().as_bytes()))).is_err() {
            return;
        }
        let mut p = pairings.lock().unwrap();
        p.phones.insert(phone_id.clone(), (phone_name.clone(), long_term));
        p.code = None;
        p.save();
        let _ = events.send(LinkEvent::Paired(phone_name.clone()));
    }
    *outgoing.lock().unwrap() = Some(tx);
    let _ = events.send(LinkEvent::Connected(phone_name));
    // writer
    let mut writer = match stream.try_clone() { Ok(s) => s, Err(_) => return };
    let wc = crypto.clone();
    std::thread::spawn(move || {
        for m in rx {
            if writer.write_all(&domain::frame(&wc.seal(m.to_string().as_bytes()))).is_err() {
                break;
            }
        }
    });
    let _ = stream.set_read_timeout(Some(Duration::from_secs(90)));
    let mut files: HashMap<String, (String, Vec<u8>)> = HashMap::new();
    while let Some(sealed) = read_frame(&mut stream) {
        let Some(plain) = crypto.open(&sealed) else { break };
        let Ok(m) = serde_json::from_slice::<Value>(&plain) else { continue };
        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        match m.get("t").and_then(Value::as_str) {
            Some("ping") => {}
            Some("file_start") => { files.insert(s("id"), (s("name"), Vec::new())); }
            Some("file_piece") => {
                if let (Some(f), Ok(bytes)) = (files.get_mut(&s("id")), B64.decode(s("data"))) {
                    f.1.extend(bytes);
                }
            }
            Some("file_end") => {
                if let Some((name, data)) = files.remove(&s("id")) {
                    if let Some(path) = save_download(&name, &data) {
                        let _ = events.send(LinkEvent::FileSaved(path));
                    }
                }
            }
            _ => { let _ = events.send(LinkEvent::Message(m)); }
        }
    }
    *outgoing.lock().unwrap() = None;
    let _ = events.send(LinkEvent::Disconnected);
}

fn save_download(name: &str, data: &[u8]) -> Option<PathBuf> {
    let dir = Command::new("xdg-user-dir").arg("DOWNLOAD").output().ok()
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Downloads"));
    let safe = domain::safe_name(name);
    let mut path = dir.join(&safe);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{n} {safe}"));
        n += 1;
    }
    std::fs::write(&path, data).ok().map(|_| path)
}

pub struct LinuxDesktop;

impl Desktop for LinuxDesktop {
    fn notify(&self, title: &str, body: &str) { notify(title, body) }
    fn clipboard_text(&self) -> Option<String> { clipboard_text() }
    fn pick_file(&self) -> Option<PathBuf> { pick_file() }
}

/// A desktop notification (notify-send), with the app shown as Tether.
pub fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send").args(["--app-name=Tether", "--icon=io.github.quezka.Tether", title, body])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// The computer's clipboard text (wl-paste on Wayland, xclip on X11).
pub fn clipboard_text() -> Option<String> {
    for (cmd, args) in [("wl-paste", vec!["--no-newline"]), ("xclip", vec!["-o", "-selection", "clipboard"])] {
        if let Ok(o) = Command::new(cmd).args(&args).output() {
            if o.status.success() {
                return Some(String::from_utf8_lossy(&o.stdout).to_string());
            }
        }
    }
    None
}

/// Pick a file with the desktop's own dialog (zenity / kdialog).
pub fn pick_file() -> Option<PathBuf> {
    for (cmd, args) in [("zenity", vec!["--file-selection"]), ("kdialog", vec!["--getopenfilename"])] {
        if let Ok(o) = Command::new(cmd).args(&args).output() {
            if o.status.success() {
                let p = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    None
}
