use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gpui::{AssetSource, SharedString};

/// Resolves an asset both during development (next to the crate) and from an
/// installed copy under `~/.local/share/escargot-native`.
pub fn asset_path(relative: &str) -> PathBuf {
    let relative = Path::new(relative);
    let mut roots = Vec::new();

    if let Ok(root) = std::env::var("ESCARGOT_NATIVE_ASSETS") {
        roots.push(PathBuf::from(root));
    }
    if let Some(data) = dirs::data_dir() {
        roots.push(data.join("escargot-native"));
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    for root in &roots {
        let candidate = root.join(relative);
        if candidate.exists() {
            return candidate;
        }
    }

    roots
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join(relative)
}

/// Loads the original MSN assets that ship next to the binary. Paths passed to
/// `img()` are relative to the crate root, e.g. `assets/img/butterfly.png`.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let full = asset_path(path);
        match std::fs::read(full) {
            Ok(bytes) => Ok(Some(Cow::Owned(bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let full = asset_path(path);
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(full)? {
            let entry = entry?;
            entries.push(SharedString::from(entry.path().to_string_lossy().to_string()));
        }
        Ok(entries)
    }
}

/// Bundled display pictures used by the demo contacts. Real MSNP11 contacts
/// do not expose a display picture here, so they receive the clean default.
pub fn avatar_for(email: &str) -> &'static str {
    let email = email.to_ascii_lowercase();
    if email.contains("alex") {
        "assets/img/avatar_duck.png"
    } else if email.contains("marina") {
        "assets/img/avatar_flower.png"
    } else if email.contains("pixel") {
        "assets/img/avatar_soccer.png"
    } else if email.contains("sam") {
        "assets/img/avatar_dog.png"
    } else {
        "assets/img/avatar_default.png"
    }
}

pub fn load_fonts(cx: &gpui::App) {
    cx.text_system()
        .add_fonts(vec![
            Cow::Borrowed(include_bytes!("../../assets/fonts/tahoma.ttf") as &[u8]),
            Cow::Borrowed(include_bytes!("../../assets/fonts/tahomabd.ttf") as &[u8]),
        ])
        .expect("bundled Tahoma fonts should load");
}
