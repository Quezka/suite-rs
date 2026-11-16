//! Passwords in the operating system's keyring (GNOME Keyring / KWallet through Secret Service on Linux, Credential
//! Manager on Windows), under the same service and account as the Python app, so both find them.

use crate::application::errors::{AppError, AppResult};
use crate::application::ports::{CredentialStore, Credentials};

pub const APP_ID: &str = "io.github.quezka.Quire";

pub struct KeyringCredentials {
    /// "classeviva" (the school register) or "sync".
    pub account: String,
}

impl KeyringCredentials {
    fn entry(&self) -> Option<keyring::Entry> {
        keyring::Entry::new(APP_ID, &self.account).ok()
    }
}

impl CredentialStore for KeyringCredentials {
    fn load(&self) -> Option<Credentials> {
        let raw = self.entry()?.get_password().ok()?;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
        Some(Credentials { username: v.get("username")?.as_str()?.into(), password: v.get("password")?.as_str()?.into() })
    }

    fn save(&self, c: &Credentials) -> AppResult {
        let raw = serde_json::json!({"username": c.username, "password": c.password}).to_string();
        self.entry().and_then(|e| e.set_password(&raw).ok()).ok_or_else(|| AppError::Storage(
            "Couldn't save the password in your system keyring. On Linux, make sure GNOME Keyring or KWallet is running.".into()))
    }

    fn clear(&self) -> AppResult {
        if let Some(e) = self.entry() {
            let _ = e.delete_credential();
        }
        Ok(())
    }
}
