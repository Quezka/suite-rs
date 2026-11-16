//! tether-relay [--port 47822]
//! pc + phone dial out, send `TETHERRELAY1 role room`; same room, both roles -> "ok\n" to each, then pipe both ways.
//! sees only ciphertext (aes-gcm end to end) + room (one-way hash of the key) -> no secrets here.
//! limits: 1 waiter per room+role (newest wins), waiter dropped after `WAIT`, max `MAX_WAITERS`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tether::domain::{parse_relay_hello, RELAY_PORT};

const WAIT: Duration = Duration::from_secs(20 * 60);
const MAX_WAITERS: usize = 2000;

type Waiting = Arc<Mutex<HashMap<(String, String), (TcpStream, Instant)>>>;

fn main() {
    let port = std::env::args().skip_while(|a| a != "--port").nth(1).and_then(|p| p.parse().ok()).unwrap_or(RELAY_PORT);
    let listener = TcpListener::bind(("0.0.0.0", port)).expect("relay port");
    eprintln!("tether-relay on :{port}");
    let waiting: Waiting = Arc::default();
    for stream in listener.incoming().flatten() {
        let w = waiting.clone();
        std::thread::spawn(move || arrive(stream, w));
    }
}

fn arrive(stream: TcpStream, waiting: Waiting) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut line = String::new();
    // byte by byte up to '\n' <- BufReader could swallow bytes meant for the partner
    let mut byte = [0u8; 1];
    let mut reader = &stream;
    while line.len() < 128 {
        match reader.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => break,
            Ok(1) => line.push(byte[0] as char),
            _ => return,
        }
    }
    let Some((role, room)) = parse_relay_hello(&line) else { return };
    let _ = stream.set_read_timeout(None);
    let other = if role == "pc" { "phone" } else { "pc" };
    let mut w = waiting.lock().unwrap();
    w.retain(|_, (s, at)| at.elapsed() < WAIT && s.peer_addr().is_ok());
    if let Some((partner, _)) = w.remove(&(room.clone(), other.to_string())) {
        drop(w);
        pipe(stream, partner);
        return;
    }
    if w.len() < MAX_WAITERS {
        if let Some((old, _)) = w.insert((room, role), (stream, Instant::now())) {
            let _ = old.shutdown(Shutdown::Both);
        }
    }
}

fn pipe(a: TcpStream, b: TcpStream) {
    let (mut a2, mut b2) = (a.try_clone().unwrap(), b.try_clone().unwrap());
    if (&a).write_all(b"ok\n").is_err() || (&b).write_all(b"ok\n").is_err() { return; }
    let t = std::thread::spawn(move || { let _ = std::io::copy(&mut b2, &mut a2); let _ = a2.shutdown(Shutdown::Both); });
    let (mut a3, mut b3) = (a, b);
    let _ = std::io::copy(&mut a3, &mut b3);
    let _ = b3.shutdown(Shutdown::Both);
    let _ = t.join();
}
