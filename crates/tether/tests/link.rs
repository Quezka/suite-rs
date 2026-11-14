//! A pretend phone pairs over the real link and sends a notification; then reconnects with the long-term key.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::channel;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};
use tether::application::{LinkEvent, PhoneLink};
use tether::domain::{self, LINK_PORT};
use tether::infrastructure::Link;

fn read(s: &mut TcpStream) -> Vec<u8> {
    let mut h = [0u8; 4];
    s.read_exact(&mut h).unwrap();
    let mut b = vec![0u8; u32::from_be_bytes(h) as usize];
    s.read_exact(&mut b).unwrap();
    b
}

fn connect(secret: &[u8], pairing: bool) -> (TcpStream, Aes256Gcm) {
    let mut s = TcpStream::connect(("127.0.0.1", LINK_PORT)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let nonce = [7u8; 16];
    let hello = json!({"t": "hello", "id": "phone1", "name": "Pixel", "nonce": B64.encode(nonce), "pairing": pairing});
    s.write_all(&domain::frame(hello.to_string().as_bytes())).unwrap();
    let reply: Value = serde_json::from_slice(&read(&mut s)).unwrap();
    let dn = B64.decode(reply["nonce"].as_str().unwrap()).unwrap();
    let key = domain::session_key(secret, &nonce, &dn);
    let c = Aes256Gcm::new_from_slice(&key).unwrap();
    (s, c)
}

fn send(s: &mut TcpStream, c: &Aes256Gcm, v: Value) {
    let iv = [3u8; 12];
    let mut out = iv.to_vec();
    out.extend(c.encrypt(Nonce::from_slice(&iv), v.to_string().as_bytes()).unwrap());
    s.write_all(&domain::frame(&out)).unwrap();
}

fn open(c: &Aes256Gcm, b: &[u8]) -> Value {
    serde_json::from_slice(&c.decrypt(Nonce::from_slice(&b[..12]), &b[12..]).unwrap()).unwrap()
}

#[test]
fn pair_then_talk_then_reconnect() {
    std::env::set_var("XDG_DATA_HOME", std::env::temp_dir().join(format!("tether-test-{}", std::process::id())));
    let (tx, rx) = channel();
    let link = Link::start(tx);
    std::thread::sleep(Duration::from_millis(300));
    let code = link.new_code();
    let (mut s, c) = connect(code.as_bytes(), true);
    send(&mut s, &c, json!({"t": "auth"}));
    let paired = open(&c, &read(&mut s));
    assert_eq!(paired["t"], "paired");
    let long_term = B64.decode(paired["key"].as_str().unwrap()).unwrap();
    send(&mut s, &c, json!({"t": "notif", "key": "k", "app": "Mail", "title": "Hi", "text": "x"}));
    let mut got = false;
    for _ in 0..20 {
        if let Ok(LinkEvent::Message(m)) = rx.recv_timeout(Duration::from_secs(2)) {
            assert_eq!(m["t"], "notif");
            got = true;
            break;
        }
    }
    assert!(got);
    assert!(link.send(json!({"t": "clip", "text": "hello phone"})));
    assert_eq!(open(&c, &read(&mut s))["text"], "hello phone");
    drop(s);
    std::thread::sleep(Duration::from_millis(200));
    // later: the long-term key, no code
    let (mut s, c) = connect(&long_term, false);
    send(&mut s, &c, json!({"t": "auth"}));
    send(&mut s, &c, json!({"t": "battery", "level": 80, "charging": true}));
    let mut ok = false;
    for _ in 0..20 {
        if let Ok(LinkEvent::Message(m)) = rx.recv_timeout(Duration::from_secs(2)) {
            if m["t"] == "battery" { ok = true; break; }
        }
    }
    assert!(ok);
    // a wrong code is refused
    let _ = link.new_code();
    let (mut bad, c2) = connect(b"wrongcode", true);
    send(&mut bad, &c2, json!({"t": "auth"}));
    let mut h = [0u8; 4];
    assert!(bad.read_exact(&mut h).is_err());
}
