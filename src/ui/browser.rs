//! Opens links in the bundled WebKitGTK window, with the system browser as a
//! last-resort fallback.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use gpui::App;

pub fn open(url: &str, cx: &mut App) {
    if std::env::var("ESCARGOT_BROWSER").ok().as_deref() == Some("system") {
        cx.open_url(url);
        return;
    }

    if let Some(helper) = helper_path() {
        if Command::new(helper)
            .env("ESCARGOT_WEBVIEW_URL", url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return;
        }
    }

    // No helper, or it could not start (missing WebKit/GL): keep the link
    // usable through the desktop's default browser.
    cx.open_url(url);
}

fn helper_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("escargot-webview");
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        let local = home.join(".local/bin/escargot-webview");
        if local.is_file() {
            return Some(local);
        }
    }

    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("escargot-webview");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}
