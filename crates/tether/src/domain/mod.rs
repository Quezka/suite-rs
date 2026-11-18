//! The protocol's rules (pure): beacons, key derivation, framing, and the messages phone and computer send.

use sha2::{Digest, Sha256};

pub const BEACON_PORT: u16 = 47820;
pub const LINK_PORT: u16 = 47821;
/// Relay (away from Wi-Fi): both sides dial out to it, it pipes bytes; it never sees a key.
pub const RELAY_PORT: u16 = 47822;
/// RFCOMM channel for the Bluetooth link (both sides use it; no SDP lookup)
pub const RFCOMM_CHANNEL: u8 = 11;
pub const RELAY_MAGIC: &str = "TETHERRELAY1";
const MAGIC: &str = "TETHER1";
/// The largest frame accepted (a file is sent in pieces below this).
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub const FILE_PIECE: usize = 1024 * 1024;

pub fn beacon(name: &str, id: &str) -> String {
    let clean: String = name.chars().filter(|c| *c != '|' && !c.is_control()).take(64).collect();
    format!("{MAGIC}|{clean}|{LINK_PORT}|{id}")
}

/// (name, port, id) from a beacon.
pub fn parse_beacon(text: &str) -> Option<(String, u16, String)> {
    let p: Vec<&str> = text.trim().split('|').collect();
    if p.len() != 4 || p[0] != MAGIC || p[1].is_empty() {
        return None;
    }
    Some((p[1].to_string(), p[2].parse().ok()?, p[3].to_string()))
}

/// The session key: SHA-256 of a label, the secret (the pairing code, later the long-term key) and both nonces.
pub fn session_key(secret: &[u8], phone_nonce: &[u8], desktop_nonce: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"tether1 session");
    h.update((secret.len() as u32).to_be_bytes());
    h.update(secret);
    h.update(phone_nonce);
    h.update(desktop_nonce);
    h.finalize().into()
}

/// Meeting point at the relay for one pairing: from the long-term key, one-way -> relay learns nothing.
pub fn relay_room(key: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"tether relay");
    h.update(key);
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// First line a side sends the relay: `TETHERRELAY1 pc|phone <room>`.
pub fn relay_hello(role: &str, room: &str) -> String {
    format!("{RELAY_MAGIC} {role} {room}\n")
}

/// (role, room) from a hello line; None if it isn't one.
pub fn parse_relay_hello(line: &str) -> Option<(String, String)> {
    let mut p = line.trim().split(' ');
    if p.next()? != RELAY_MAGIC { return None; }
    let role = p.next()?;
    let room = p.next()?;
    if !matches!(role, "pc" | "phone") || room.len() != 32 || !room.bytes().all(|b| b.is_ascii_hexdigit()) || p.next().is_some() {
        return None;
    }
    Some((role.into(), room.into()))
}

/// "host" or "host:port" -> (host, port), default port RELAY_PORT.
pub fn relay_address(text: &str) -> Option<(String, u16)> {
    let t = text.trim();
    if t.is_empty() { return None; }
    match t.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.parse::<u16>().is_ok() => Some((h.into(), p.parse().ok()?)),
        _ => Some((t.into(), RELAY_PORT)),
    }
}

/// The pairing code shown on the computer: 8 characters without look-alikes (about 40 bits).
pub fn pairing_code(random: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    random.iter().take(8).map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char).collect()
}

pub fn normal_code(typed: &str) -> String {
    typed.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

/// A frame on the wire: 4-byte big-endian length, then the bytes.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(payload);
    out
}

/// The length a frame header announces, if acceptable.
pub fn frame_len(header: [u8; 4]) -> Option<usize> {
    let n = u32::from_be_bytes(header) as usize;
    (n > 0 && n <= MAX_FRAME).then_some(n)
}

/// A file name safe to save under the downloads folder.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().filter(|c| !c.is_control() && *c != ':').collect();
    let clean = clean.trim_start_matches('.').trim();
    if clean.is_empty() { "file".into() } else { clean.chars().take(120).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacons_and_codes() {
        assert_eq!(parse_beacon(&beacon("qp|c", "abc")), Some(("qpc".into(), LINK_PORT, "abc".into())));
        assert!(parse_beacon("TETHER1|x|y|z").is_none());
        assert_eq!(normal_code(" AB-cd 23 "), "abcd23");
        assert_eq!(pairing_code(&[1; 8]).len(), 8);
    }

    #[test]
    fn keys_depend_on_everything() {
        let k = session_key(b"code", b"p", b"d");
        assert_ne!(k, session_key(b"code", b"p", b"e"));
        assert_ne!(k, session_key(b"cod", b"ep", b"d"));
    }

    #[test]
    fn frames_and_names() {
        let f = frame(b"hi");
        assert_eq!(frame_len([f[0], f[1], f[2], f[3]]), Some(2));
        assert_eq!(frame_len([0, 0, 0, 0]), None);
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name(".hidden"), "hidden");
        assert_eq!(safe_name(""), "file");
    }
}

#[cfg(test)]
mod relay_tests {
    use super::*;

    #[test]
    fn room_is_stable_hex_and_hello_round_trips() {
        let room = relay_room(&[7u8; 32]);
        assert_eq!(room.len(), 32);
        assert_eq!(room, relay_room(&[7u8; 32]));
        assert_ne!(room, relay_room(&[8u8; 32]));
        assert_eq!(parse_relay_hello(&relay_hello("phone", &room)), Some(("phone".into(), room.clone())));
        assert_eq!(parse_relay_hello("TETHERRELAY1 admin abc"), None);
        assert_eq!(relay_address("home.example:9000"), Some(("home.example".into(), 9000)));
        assert_eq!(relay_address("home.example"), Some(("home.example".into(), RELAY_PORT)));
        assert_eq!(relay_address(" "), None);
    }

    #[test]
    fn room_matches_the_phone() {
        // = ProtocolTest.relayRoom (kotlin)
        assert_eq!(relay_room(&[1u8; 32]), "11f9c4d690fca1e143bcde677dac9fde");
    }
}
