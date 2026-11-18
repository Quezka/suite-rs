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

pub mod bluetooth;
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
    /// relay "host[:port]" for away from wi-fi; None = off
    pub relay: Option<String>,
    /// also listen over Bluetooth (RFCOMM), for when there is no shared wi-fi
    pub bluetooth: bool,
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
        let relay = v.get("relay").and_then(Value::as_str).map(str::to_string).filter(|r| !r.trim().is_empty());
        let bluetooth = v.get("bluetooth").and_then(Value::as_bool).unwrap_or(false);
        let p = Self { phones, code: None, computer_id, relay, bluetooth };
        p.save();
        p
    }

    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let phones: serde_json::Map<String, Value> = self.phones.iter()
            .map(|(id, (name, key))| (id.clone(), json!({"name": name, "key": B64.encode(key)}))).collect();
        let path = dir.join("paired.json");
        let _ = std::fs::write(&path, json!({"id": self.computer_id, "phones": phones, "relay": self.relay, "bluetooth": self.bluetooth}).to_string());
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

fn read_frame<R: Read>(s: &mut R) -> Option<Vec<u8>> {
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
        let (p, o, ev) = (pairings.clone(), outgoing.clone(), events.clone());
        std::thread::spawn(move || {
            let Ok(listener) = TcpListener::bind(("0.0.0.0", LINK_PORT)) else { return };
            for stream in listener.incoming().flatten() {
                let (p, o, ev) = (p.clone(), o.clone(), ev.clone());
                std::thread::spawn(move || serve(stream, p, o, ev));
            }
        });
        // relay: one waiting connection per paired phone (phone dials the same room when off our wi-fi)
        let (p, o, ev) = (pairings.clone(), outgoing.clone(), events.clone());
        std::thread::spawn(move || relay_supervisor(p, o, ev));
        // Bluetooth: listen on RFCOMM when the user turned it on (no shared wi-fi needed)
        if pairings.lock().unwrap().bluetooth {
            let (p, o) = (pairings.clone(), outgoing.clone());
            std::thread::spawn(move || bluetooth::listen(p, o, events));
        }
        Self { pairings, outgoing }
    }

}

fn relay_supervisor(pairings: Arc<Mutex<Pairings>>, outgoing: Arc<Mutex<Option<Sender<Value>>>>, events: Sender<LinkEvent>) {
    let active: Arc<Mutex<std::collections::HashSet<String>>> = Arc::default();
    loop {
        let (relay, phones) = {
            let p = pairings.lock().unwrap();
            (p.relay.clone(), p.phones.iter().map(|(id, (_, k))| (id.clone(), k.clone())).collect::<Vec<_>>())
        };
        if let Some((host, port)) = relay.as_deref().and_then(domain::relay_address) {
            for (id, key) in phones {
                if !active.lock().unwrap().insert(id.clone()) { continue; }
                let (p, o, ev, a, host) = (pairings.clone(), outgoing.clone(), events.clone(), active.clone(), host.clone());
                std::thread::spawn(move || {
                    if let Some(stream) = relay_wait(&host, port, &domain::relay_room(&key)) {
                        serve(stream, p, o, ev);
                    } else {
                        std::thread::sleep(Duration::from_secs(10)); // relay down -> no busy loop
                    }
                    a.lock().unwrap().remove(&id);
                });
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// Dial the relay, wait (up to ~19 min) for the phone to join our room; the stream then = a link like any other.
fn relay_wait(host: &str, port: u16, room: &str) -> Option<TcpStream> {
    use std::net::ToSocketAddrs;
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).ok()?;
    s.write_all(domain::relay_hello("pc", room).as_bytes()).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(19 * 60)));
    let mut ok = [0u8; 3];
    s.read_exact(&mut ok).ok()?;
    (&ok == b"ok\n").then_some(s)
}

impl PhoneLink for Link {
    fn relay(&self) -> Option<String> {
        self.pairings.lock().unwrap().relay.clone()
    }

    fn set_relay(&self, address: Option<String>) {
        let mut p = self.pairings.lock().unwrap();
        p.relay = address.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
        p.save();
    }

    fn bluetooth(&self) -> bool {
        self.pairings.lock().unwrap().bluetooth
    }

    fn set_bluetooth(&self, on: bool) {
        let mut p = self.pairings.lock().unwrap();
        p.bluetooth = on;
        p.save();
        // the RFCOMM listener starts at launch; a toggle takes effect next time Tether opens
    }

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

/// The wifi link: a TCP stream with timeouts, handed to the shared `serve_stream` with a cloned write handle.
fn serve(stream: TcpStream, pairings: Arc<Mutex<Pairings>>, outgoing: Arc<Mutex<Option<Sender<Value>>>>, events: Sender<LinkEvent>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(90)));
    let Ok(writer) = stream.try_clone() else { return };
    serve_stream(stream, writer, pairings, outgoing, events);
}

/// The link over any stream: TCP on wifi, an RFCOMM socket on Bluetooth. `stream` reads and writes the
/// handshake; `writer_half` is a second handle (a cloned socket, or a dup'd fd) for the background sender.
fn serve_stream<S, W>(mut stream: S, mut writer_half: W, pairings: Arc<Mutex<Pairings>>, outgoing: Arc<Mutex<Option<Sender<Value>>>>, events: Sender<LinkEvent>)
where
    S: Read + Write,
    W: Write + Send + 'static,
{
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
    // writer: the background sender uses the second handle (wifi: a cloned socket; BT: a dup'd fd)
    let wc = crypto.clone();
    std::thread::spawn(move || {
        for m in rx {
            if writer_half.write_all(&domain::frame(&wc.seal(m.to_string().as_bytes()))).is_err() {
                break;
            }
        }
    });
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
    fn autostart_enabled(&self) -> bool { login_item().enabled() }
    fn set_autostart(&self, on: bool) -> Result<(), String> { set_autostart(on) }
}

/// login entry (linux only: tether = unix socket app); rules @ `quezka_kit::autostart`
fn login_item() -> quezka_kit::autostart::LoginItem {
    quezka_kit::autostart::LoginItem {
        app_id: "io.github.quezka.Tether".into(), name: "Tether".into(), comment: "Your phone on your computer".into(),
        command: vec!["tether".into(), "--background".into()], config_dir: None,
    }
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    login_item().set_enabled(on).map_err(|_| "Couldn't change starting with the computer.".to_string())
}

/// First start ever: switch starting at login on (the link only helps if it's always there). Once.
pub fn autostart_on_first_run() {
    let marker = data_dir().join("autostart-chosen");
    if !marker.exists() {
        let _ = set_autostart(true);
        let _ = std::fs::create_dir_all(data_dir());
        let _ = std::fs::write(marker, "");
    }
}

/// One Tether per user. `Ok(requests)`: this is the first; `requests` gets a () each time another one starts.
/// `Err(())`: one is already running and was asked to show its window.
pub fn single_instance() -> Result<Receiver<()>, ()> {
    use std::os::unix::net::{UnixListener, UnixStream};
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(data_dir);
    let path = dir.join("tether.sock");
    if let Ok(mut other) = UnixStream::connect(&path) {
        let _ = other.write_all(b"show");
        return Err(());
    }
    let _ = std::fs::remove_file(&path); // stale socket from a crash
    let (tx, rx) = channel();
    if let Ok(listener) = UnixListener::bind(&path) {
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                drop(stream);
                if tx.send(()).is_err() { break; }
            }
        });
    }
    Ok(rx)
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
