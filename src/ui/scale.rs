//! Shared design-pixel zoom for native views and keyboard shortcuts.
use crate::config;
use gpui::{App, KeyBinding, Pixels, Window};
use std::sync::atomic::{AtomicU16, Ordering};

gpui::actions!(appearance, [ZoomIn, ZoomOut, ResetZoom]);
static PERCENT: AtomicU16 = AtomicU16::new(100);

pub fn adjusted_percent(percent: u16, delta: i16) -> u16 {
    (i32::from(percent) + i32::from(delta)).clamp(75, 200) as u16
}

pub fn percent() -> u16 {
    PERCENT.load(Ordering::Relaxed)
}

pub fn set_percent(percent: u16) {
    PERCENT.store(percent.clamp(75, 200), Ordering::Relaxed);
}

/// Convert design pixels without changing native window coordinates.
pub fn px(value: f32) -> Pixels {
    gpui::px(value * f32::from(percent()) / 100.)
}

pub fn apply(window: &mut Window) {
    window.set_rem_size(px(16.));
}

fn set_zoom(percent: u16, cx: &mut App) {
    set_percent(percent);
    let mut settings = config::load();
    settings.ui_zoom = self::percent();
    let _ = config::save(&settings);
    cx.refresh_windows();
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-+", ZoomIn, None),
        KeyBinding::new("ctrl-=", ZoomIn, None),
        KeyBinding::new("ctrl-shift-=", ZoomIn, None),
        KeyBinding::new("ctrl--", ZoomOut, None),
        KeyBinding::new("ctrl-0", ResetZoom, None),
    ]);
    cx.on_action(|_: &ZoomIn, cx| set_zoom(adjusted_percent(percent(), 25), cx));
    cx.on_action(|_: &ZoomOut, cx| set_zoom(adjusted_percent(percent(), -25), cx));
    cx.on_action(|_: &ResetZoom, cx| set_zoom(100, cx));
}
