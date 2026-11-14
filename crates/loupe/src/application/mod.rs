//! What Loupe does, through ports the infrastructure fills in.

use crate::domain::{self, Peer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoupeError(pub String);

/// Running programs (ffmpeg, FreeRDP) and system setup steps.
pub trait System: Send + Sync {
    /// Start a long-running program; returns a handle to stop it.
    fn start(&self, program: &str, args: &[String]) -> Result<u32, LoupeError>;
    fn stop(&self, handle: u32);
    fn running(&self, handle: u32) -> bool;
    /// The first camera of this computer (e.g. /dev/video0), if any.
    fn camera(&self) -> Option<String>;
    /// Whether a program is installed.
    fn has(&self, program: &str) -> bool;
    /// Load the virtual webcam (asks for the password once).
    fn load_virtual_camera(&self, options: &[String]) -> Result<(), LoupeError>;
    fn virtual_camera_ready(&self) -> bool;
    /// Turn GNOME's remote desktop on in "extend" mode with this password, or off.
    fn share_screen(&self, code: Option<&str>) -> Result<(), LoupeError>;
    fn computer_name(&self) -> String;
    fn random(&self) -> Vec<u8>;
}

/// Hearing and sending beacons on the local network.
pub trait Neighbours: Send + Sync {
    fn announce(&self, beacon: &str);
    fn heard(&self) -> Vec<(String, String)>; // (text, address)
}

pub struct LoupeService {
    system: Box<dyn System>,
    net: Box<dyn Neighbours>,
    sending: Option<(u32, String)>,
    receiving: Option<u32>,
    viewing: Option<u32>,
    pub screen_code: Option<String>,
}

impl LoupeService {
    pub fn new(system: Box<dyn System>, net: Box<dyn Neighbours>) -> Self {
        Self { system, net, sending: None, receiving: None, viewing: None, screen_code: None }
    }

    pub fn name(&self) -> String {
        self.system.computer_name()
    }

    pub fn has_camera(&self) -> bool {
        self.system.camera().is_some()
    }

    /// Say we're here (call every couple of seconds) and list who else is.
    pub fn tick(&mut self) -> Vec<Peer> {
        self.net.announce(&domain::beacon(&self.name(), self.has_camera(), self.screen_code.is_some()));
        let mut peers: Vec<Peer> = Vec::new();
        for (text, address) in self.net.heard() {
            if let Some(p) = domain::parse_beacon(&text, &address) {
                if p.name != self.name() && !peers.iter().any(|q| q.address == p.address) {
                    peers.push(p);
                }
            }
        }
        for slot in [&mut self.receiving, &mut self.viewing] {
            if slot.is_some_and(|h| !self.system.running(h)) {
                *slot = None;
            }
        }
        if self.sending.as_ref().is_some_and(|(h, _)| !self.system.running(*h)) {
            self.sending = None;
        }
        peers.sort_by(|a, b| a.name.cmp(&b.name));
        peers
    }

    pub fn sending_to(&self) -> Option<&str> {
        self.sending.as_ref().map(|(_, host)| host.as_str())
    }
    pub fn receiving(&self) -> bool {
        self.receiving.is_some()
    }
    pub fn viewing(&self) -> bool {
        self.viewing.is_some()
    }

    pub fn send_camera(&mut self, host: &str) -> Result<(), LoupeError> {
        self.stop_sending();
        let camera = self.system.camera().ok_or_else(|| LoupeError("This computer has no camera.".into()))?;
        self.need("ffmpeg")?;
        let h = self.system.start("ffmpeg", &domain::sender_args(&camera, host))?;
        self.sending = Some((h, host.to_string()));
        Ok(())
    }

    pub fn stop_sending(&mut self) {
        if let Some((h, _)) = self.sending.take() {
            self.system.stop(h);
        }
    }

    pub fn receive_camera(&mut self) -> Result<(), LoupeError> {
        self.stop_receiving();
        self.need("ffmpeg")?;
        if !self.system.virtual_camera_ready() {
            self.system.load_virtual_camera(&domain::loopback_options())?;
        }
        self.receiving = Some(self.system.start("ffmpeg", &domain::receiver_args())?);
        Ok(())
    }

    pub fn stop_receiving(&mut self) {
        if let Some(h) = self.receiving.take() {
            self.system.stop(h);
        }
    }

    /// Let a laptop extend this screen: a fresh pairing code, shown here and typed on the laptop.
    pub fn share_screen(&mut self, on: bool) -> Result<(), LoupeError> {
        if on {
            let code = domain::pairing_code(&self.system.random());
            self.system.share_screen(Some(&code))?;
            self.screen_code = Some(code);
        } else {
            self.system.share_screen(None)?;
            self.screen_code = None;
        }
        Ok(())
    }

    /// Become the second screen of `host` (full screen; leave with the remote desktop's own keys).
    pub fn view_screen(&mut self, host: &str, code: &str) -> Result<(), LoupeError> {
        let client = ["xfreerdp3", "sdl-freerdp3", "xfreerdp"].into_iter().find(|p| self.system.has(p))
            .ok_or_else(|| LoupeError("Install FreeRDP first: sudo apt install freerdp3-x11".into()))?;
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        self.viewing = Some(self.system.start(client, &domain::rdp_client_args(host, &code))?);
        Ok(())
    }

    pub fn stop_viewing(&mut self) {
        if let Some(h) = self.viewing.take() {
            self.system.stop(h);
        }
    }

    fn need(&self, program: &str) -> Result<(), LoupeError> {
        if self.system.has(program) { Ok(()) } else { Err(LoupeError(format!("Install {program} first: sudo apt install {program}"))) }
    }

    /// Everything off (the window is closing).
    pub fn shutdown(&mut self) {
        self.stop_sending();
        self.stop_receiving();
        self.stop_viewing();
        if self.screen_code.is_some() {
            let _ = self.share_screen(false);
        }
    }
}
