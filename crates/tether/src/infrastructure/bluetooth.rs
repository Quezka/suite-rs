//! Bluetooth link (RFCOMM), for when phone and computer share no wi-fi. Same handshake + frames as wifi; the
//! phone dials the computer's MAC on a fixed channel. Linux only (BlueZ); untested on real hardware.
//! ! needs the computer discoverable + the phone already Bluetooth-paired (OS-level) with it.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::application::LinkEvent;
use crate::domain::RFCOMM_CHANNEL;
use super::Pairings;

#[cfg(not(target_os = "linux"))]
pub fn listen(_: Arc<Mutex<Pairings>>, _: Arc<Mutex<Option<Sender<Value>>>>, _: Sender<LinkEvent>) {}

#[cfg(target_os = "linux")]
pub fn listen(pairings: Arc<Mutex<Pairings>>, outgoing: Arc<Mutex<Option<Sender<Value>>>>, events: Sender<LinkEvent>) {
    use std::os::fd::FromRawFd;
    let fd = match imp::listen_rfcomm(RFCOMM_CHANNEL) {
        Ok(fd) => fd,
        Err(_) => return, // no adapter, no permission: Bluetooth just stays off
    };
    loop {
        let client = match imp::accept(fd) {
            Ok(c) => c,
            Err(_) => { std::thread::sleep(std::time::Duration::from_secs(2)); continue }
        };
        // two owned handles over the same connection: one to read + write the handshake, one for the sender thread
        let dup = unsafe { libc::dup(client) };
        if dup < 0 { unsafe { libc::close(client); } continue; }
        let stream = unsafe { std::fs::File::from_raw_fd(client) };
        let writer = unsafe { std::fs::File::from_raw_fd(dup) };
        let (p, o, ev) = (pairings.clone(), outgoing.clone(), events.clone());
        std::thread::spawn(move || super::serve_stream(stream, writer, p, o, ev));
    }
}

#[cfg(target_os = "linux")]
mod imp {
    #[repr(C, packed)]
    struct SockaddrRc {
        rc_family: libc::sa_family_t,
        rc_bdaddr: [u8; 6],
        rc_channel: u8,
    }
    const AF_BLUETOOTH: libc::c_int = 31;
    const BTPROTO_RFCOMM: libc::c_int = 3;

    pub fn listen_rfcomm(channel: u8) -> Result<libc::c_int, ()> {
        unsafe {
            let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM);
            if fd < 0 { return Err(()); }
            let addr = SockaddrRc { rc_family: AF_BLUETOOTH as libc::sa_family_t, rc_bdaddr: [0; 6], rc_channel: channel };
            let r = libc::bind(fd, &addr as *const _ as *const libc::sockaddr, std::mem::size_of::<SockaddrRc>() as libc::socklen_t);
            if r < 0 || libc::listen(fd, 2) < 0 { libc::close(fd); return Err(()); }
            Ok(fd)
        }
    }

    pub fn accept(fd: libc::c_int) -> Result<libc::c_int, ()> {
        let c = unsafe { libc::accept(fd, std::ptr::null_mut(), std::ptr::null_mut()) };
        if c < 0 { Err(()) } else { Ok(c) }
    }
}
