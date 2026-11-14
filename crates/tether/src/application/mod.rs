//! What the computer knows about the phone, updated from the messages it sends (JSON, see PROTOCOL.md).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// What the link tells the window.
pub enum LinkEvent {
    Connected(String),
    Disconnected,
    Paired(String),
    Message(Value),
    FileSaved(PathBuf),
}

/// The connection to the phone (infrastructure: TCP + AES-GCM).
pub trait PhoneLink {
    fn send(&self, message: Value) -> bool;
    fn new_code(&self) -> String;
    fn forget(&self, id: &str);
    /// (id, name) of the paired phones.
    fn paired(&self) -> Vec<(String, String)>;
    fn send_file(&self, path: &Path) -> Result<(), String>;
}

/// The computer's own side: notifications, clipboard, the file picker.
pub trait Desktop {
    fn notify(&self, title: &str, body: &str);
    fn clipboard_text(&self) -> Option<String>;
    fn pick_file(&self) -> Option<PathBuf>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhoneNotification {
    pub key: String,
    pub app: String,
    pub title: String,
    pub text: String,
    pub can_reply: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sms {
    pub incoming: bool,
    pub body: String,
    pub at: i64, // ms since 1970
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thread {
    pub address: String,
    pub name: String,
    pub messages: Vec<Sms>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    None,
    Ringing { number: String, name: String },
    Talking { number: String, name: String },
}

/// What a message from the phone means for the person at the computer (shown as a desktop notification).
#[derive(Clone, Debug, PartialEq)]
pub enum Alert {
    Notification(PhoneNotification),
    Sms { name: String, body: String },
    Ringing { name: String },
    FileSaved { name: String },
    Clipboard(String),
}

#[derive(Default)]
pub struct Phone {
    pub name: String,
    pub connected: bool,
    pub battery: Option<(u8, bool)>,
    pub notifications: Vec<PhoneNotification>,
    pub threads: Vec<Thread>,
    pub call: Option<Call>,
    pub clipboard: Option<String>,
}

impl Phone {
    /// Apply one message from the phone; returns what to tell the user, if anything.
    pub fn apply(&mut self, m: &Value) -> Option<Alert> {
        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        match m.get("t").and_then(Value::as_str)? {
            "battery" => {
                self.battery = Some((m.get("level")?.as_u64()?.min(100) as u8, m.get("charging").and_then(Value::as_bool).unwrap_or(false)));
                None
            }
            "notif" => {
                let n = PhoneNotification { key: s("key"), app: s("app"), title: s("title"), text: s("text"),
                    can_reply: m.get("reply").and_then(Value::as_bool).unwrap_or(false) };
                self.notifications.retain(|x| x.key != n.key);
                self.notifications.insert(0, n.clone());
                self.notifications.truncate(200);
                Some(Alert::Notification(n))
            }
            "notif_removed" => {
                let key = s("key");
                self.notifications.retain(|x| x.key != key);
                None
            }
            "sms_list" => {
                self.threads = m.get("threads")?.as_array()?.iter().filter_map(thread).collect();
                None
            }
            "sms_in" => {
                let (address, name) = (s("addr"), s("name"));
                let sms = Sms { incoming: true, body: s("body"), at: m.get("at").and_then(Value::as_i64).unwrap_or(0) };
                self.add_sms(&address, &name, sms.clone());
                Some(Alert::Sms { name: if name.is_empty() { address } else { name }, body: sms.body })
            }
            "call" => {
                let (number, name) = (s("number"), s("name"));
                let shown = if name.is_empty() { number.clone() } else { name.clone() };
                self.call = Some(match s("state").as_str() {
                    "ringing" => Call::Ringing { number, name },
                    "offhook" => Call::Talking { number, name },
                    _ => Call::None,
                });
                matches!(self.call, Some(Call::Ringing { .. })).then_some(Alert::Ringing { name: shown })
            }
            "clip" => {
                let text = s("text");
                self.clipboard = Some(text.clone());
                Some(Alert::Clipboard(text))
            }
            _ => None,
        }
    }

    pub fn add_sms(&mut self, address: &str, name: &str, sms: Sms) {
        if let Some(t) = self.threads.iter_mut().find(|t| t.address == address) {
            t.messages.push(sms);
        } else {
            self.threads.insert(0, Thread { address: address.into(), name: name.into(), messages: vec![sms] });
        }
        self.threads.sort_by_key(|t| std::cmp::Reverse(t.messages.last().map(|m| m.at).unwrap_or(0)));
    }
}

fn thread(v: &Value) -> Option<Thread> {
    let messages = v.get("messages")?.as_array()?.iter().filter_map(|m| Some(Sms {
        incoming: m.get("in")?.as_bool()?, body: m.get("body")?.as_str()?.to_string(), at: m.get("at")?.as_i64()? })).collect();
    Some(Thread { address: v.get("addr")?.as_str()?.into(), name: v.get("name").and_then(Value::as_str).unwrap_or_default().into(), messages })
}

/// Messages the computer sends to the phone.
pub mod to_phone {
    use super::*;
    pub fn reply(key: &str, text: &str) -> Value { json!({"t": "reply", "key": key, "text": text}) }
    pub fn dismiss(key: &str) -> Value { json!({"t": "dismiss", "key": key}) }
    pub fn sms_send(address: &str, body: &str) -> Value { json!({"t": "sms_send", "addr": address, "body": body}) }
    pub fn sms_get() -> Value { json!({"t": "sms_get"}) }
    pub fn answer() -> Value { json!({"t": "call_answer"}) }
    pub fn reject() -> Value { json!({"t": "call_reject"}) }
    pub fn clip(text: &str) -> Value { json!({"t": "clip", "text": text}) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phone_state_follows_its_messages() {
        let mut p = Phone::default();
        let a = p.apply(&json!({"t": "notif", "key": "k1", "app": "WhatsApp", "title": "Sam", "text": "hi", "reply": true}));
        assert!(matches!(a, Some(Alert::Notification(_))));
        p.apply(&json!({"t": "notif", "key": "k1", "app": "WhatsApp", "title": "Sam", "text": "hi again"}));
        assert_eq!(p.notifications.len(), 1);
        p.apply(&json!({"t": "notif_removed", "key": "k1"}));
        assert!(p.notifications.is_empty());
        p.apply(&json!({"t": "sms_in", "addr": "+39 1", "name": "Mum", "body": "dinner?", "at": 5}));
        assert_eq!(p.threads[0].messages[0].body, "dinner?");
        assert_eq!(p.apply(&json!({"t": "call", "state": "ringing", "number": "+39 1", "name": "Mum"})), Some(Alert::Ringing { name: "Mum".into() }));
        p.apply(&json!({"t": "call", "state": "idle"}));
        assert_eq!(p.call, Some(Call::None));
        assert_eq!(p.apply(&json!({"t": "nope"})), None);
    }
}
