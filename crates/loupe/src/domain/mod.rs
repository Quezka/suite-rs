//! The rules: how computers running Loupe find each other, the commands that move the picture, and the
//! second-screen pairing code. Pure: no network, no processes.

/// UDP port the beacons go to (broadcast on the local network every couple of seconds).
pub const BEACON_PORT: u16 = 47810;
/// UDP port the camera stream arrives on.
pub const CAMERA_PORT: u16 = 47811;
/// The virtual webcam's device number (/dev/video47) and the name other apps show.
pub const CAMERA_DEVICE: u32 = 47;
pub const CAMERA_LABEL: &str = "Loupe camera";
pub const RDP_USER: &str = "loupe";
const MAGIC: &str = "LOUPE1";

/// Another computer running Loupe, as its beacon describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub name: String,
    pub address: String,
    pub has_camera: bool,
    /// It lets another computer extend its screen (GNOME's remote desktop in "extend" mode is on).
    pub shares_screen: bool,
}

/// The beacon this computer sends: `LOUPE1|name|camera|screen`.
pub fn beacon(name: &str, has_camera: bool, shares_screen: bool) -> String {
    let clean: String = name.chars().filter(|c| *c != '|' && !c.is_control()).take(64).collect();
    format!("{MAGIC}|{clean}|{}|{}", u8::from(has_camera), u8::from(shares_screen))
}

/// A beacon heard from `address`; None for anything that isn't one.
pub fn parse_beacon(text: &str, address: &str) -> Option<Peer> {
    let mut parts = text.trim().split('|');
    if parts.next()? != MAGIC {
        return None;
    }
    let name = parts.next()?.to_string();
    let camera = parts.next()? == "1";
    let screen = parts.next()? == "1";
    if name.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(Peer { name, address: address.to_string(), has_camera: camera, shares_screen: screen })
}

/// ffmpeg on the laptop: its camera, H.264 tuned for delay, to the other computer.
pub fn sender_args(camera: &str, host: &str) -> Vec<String> {
    [
        "-hide_banner", "-loglevel", "error", "-f", "v4l2", "-framerate", "30", "-video_size", "1280x720", "-i", camera,
        "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-g", "30", "-b:v", "4M", "-f", "mpegts",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([format!("udp://{host}:{CAMERA_PORT}?pkt_size=1316")])
    .collect()
}

/// ffmpeg on the receiving computer: the stream into the virtual webcam.
pub fn receiver_args() -> Vec<String> {
    let input = format!("udp://0.0.0.0:{CAMERA_PORT}?fifo_size=1000000&overrun_nonfatal=1");
    [
        "-hide_banner", "-loglevel", "error", "-fflags", "nobuffer", "-flags", "low_delay", "-i", &input,
        "-pix_fmt", "yuv420p", "-f", "v4l2", &device_path(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn device_path() -> String {
    format!("/dev/video{CAMERA_DEVICE}")
}

/// modprobe options for the virtual webcam.
pub fn loopback_options() -> Vec<String> {
    vec![
        "v4l2loopback".into(),
        format!("video_nr={CAMERA_DEVICE}"),
        format!("card_label={CAMERA_LABEL}"),
        "exclusive_caps=1".into(),
    ]
}

/// FreeRDP on the laptop: full screen on the other computer's virtual monitor, sized to this screen.
pub fn rdp_client_args(host: &str, code: &str) -> Vec<String> {
    vec![
        format!("/v:{host}"),
        format!("/u:{RDP_USER}"),
        format!("/p:{code}"),
        "/cert:ignore".into(),
        "/f".into(),
        "/dynamic-resolution".into(),
        "/gfx".into(),
        "+clipboard".into(),
    ]
}

/// The second-screen pairing code (the remote desktop password): 8 letters and digits, no look-alikes, from
/// random bytes given by the caller.
pub fn pairing_code(random: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    random.iter().take(8).map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacons_go_both_ways_and_ignore_strangers() {
        let b = beacon("laptop|x", true, false);
        assert_eq!(b, "LOUPE1|laptopx|1|0");
        let p = parse_beacon(&b, "192.168.1.20").unwrap();
        assert_eq!((p.name.as_str(), p.has_camera, p.shares_screen), ("laptopx", true, false));
        assert!(parse_beacon("HELLO|x|1|1", "a").is_none());
        assert!(parse_beacon("LOUPE1||1|1", "a").is_none());
        assert!(parse_beacon("LOUPE1|a|1|1|extra", "a").is_none());
    }

    #[test]
    fn commands_point_at_the_right_places() {
        let s = sender_args("/dev/video0", "10.0.0.2");
        assert_eq!(s.last().unwrap(), "udp://10.0.0.2:47811?pkt_size=1316");
        assert!(s.contains(&"/dev/video0".to_string()));
        let r = receiver_args();
        assert_eq!(r.last().unwrap(), "/dev/video47");
        assert!(rdp_client_args("h", "abc").contains(&"/p:abc".to_string()));
        assert_eq!(loopback_options()[1], "video_nr=47");
    }

    #[test]
    fn pairing_codes_are_eight_easy_characters() {
        let code = pairing_code(&[0, 1, 2, 3, 250, 251, 252, 253, 9]);
        assert_eq!(code.len(), 8);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric() && !"il1o0".contains(c)));
    }
}
