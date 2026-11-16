//! where quire keeps data per platform (= python `infrastructure/paths.py`)

use std::path::PathBuf;

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env_dir(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_default()
}

/// windows `%APPDATA%\Quire`, mac `~/Library/Application Support/Quire`, else `$XDG_DATA_HOME/Quire`; made if missing
pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        env_dir("APPDATA").unwrap_or_else(|| home().join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local").join("share"))
    };
    let folder = base.join("Quire");
    let _ = std::fs::create_dir_all(&folder);
    folder
}
