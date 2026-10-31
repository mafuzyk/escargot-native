//! Small user settings store. The password goes to the Secret Service when
//! one is available; otherwise it falls back to a 0600 file under
//! `~/.local/share/escargot-native`, only when the user explicitly asks to be
//! remembered.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub remember_me: bool,
    #[serde(default)]
    pub remember_password: bool,
    #[serde(default)]
    pub auto_sign_in: bool,
    #[serde(default)]
    pub wallpaper: Option<String>,
    #[serde(default)]
    pub interface_wallpaper: Option<String>,
    #[serde(default = "default_zoom")]
    pub ui_zoom: u16,
    #[serde(default)]
    pub night: bool,
    #[serde(default = "default_single_window")]
    pub single_window: bool,
}

fn default_zoom() -> u16 { 100 }

fn default_single_window() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            email: None,
            remember_me: false,
            remember_password: false,
            auto_sign_in: false,
            wallpaper: None,
            interface_wallpaper: None,
            ui_zoom: 100,
            night: false,
            single_window: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPassword {
    email: String,
    password: String,
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("escargot-native").join("config.toml"))
}

fn credentials_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("escargot-native").join("credentials.toml"))
}

pub fn load() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(config: &Config) -> std::io::Result<()> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(config).unwrap_or_default();
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

pub fn forget_email() {
    let mut config = load();
    config.email = None;
    config.remember_me = false;
    config.remember_password = false;
    config.auto_sign_in = false;
    let _ = save(&config);
}

pub fn store_password(email: &str, password: &str) {
    if keyring_store(email, password) {
        if let Some(path) = credentials_path() {
            let _ = fs::remove_file(path);
        }
        return;
    }

    let Some(path) = credentials_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let stored = StoredPassword {
        email: email.to_string(),
        password: password.to_string(),
    };
    if let Ok(text) = toml::to_string(&stored) {
        let _ = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut file| file.write_all(text.as_bytes()));
    }
}

pub fn load_password(email: &str) -> Option<String> {
    if let Some(password) = keyring_lookup(email) {
        return Some(password);
    }

    let path = credentials_path()?;
    let text = fs::read_to_string(path).ok()?;
    let stored: StoredPassword = toml::from_str(&text).ok()?;
    if stored.email.eq_ignore_ascii_case(email) {
        Some(stored.password)
    } else {
        None
    }
}

pub fn clear_password(email: &str) {
    let _ = Command::new("secret-tool")
        .args(["clear", "service", "escargot-native", "account", email])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Some(path) = credentials_path() {
        let _ = fs::remove_file(path);
    }
}

fn keyring_store(email: &str, password: &str) -> bool {
    let Ok(mut child) = Command::new("secret-tool")
        .args([
            "store",
            "--label",
            "Escargot Native",
            "service",
            "escargot-native",
            "account",
            email,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(password.as_bytes()).is_err() {
            return false;
        }
    }
    child.wait().map(|status| status.success()).unwrap_or(false)
}

fn keyring_lookup(email: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(["lookup", "service", "escargot-native", "account", email])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let password = String::from_utf8_lossy(&output.stdout)
        .trim_end_matches(['\r', '\n'])
        .to_string();
    (!password.is_empty()).then_some(password)
}
