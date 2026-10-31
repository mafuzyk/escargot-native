//! Visual tokens taken from the original Windows XP-era MSN Messenger 7.5
//! screens and the assets extracted from msnmsgrexe. The palette has two
//! modes: the daylight Aero glass of 7.5 and the night variant inspired by
//! Frutiger Night.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{Background, FontWeight, Hsla, linear_color_stop, linear_gradient, rgb, rgba};

pub const FONT: &str = "Tahoma";

pub const TITLE_TOP: u32 = 0x8c98b2;
pub const TITLE_MID: u32 = 0xc4e8f7;
pub const TITLE_LOW: u32 = 0xffffff;
pub const TITLE_EDGE: u32 = 0x7d8ba8;
pub const TITLE_TEXT: u32 = 0x1c3f7c;

pub const LINK_BLUE: u32 = 0x0a3d91;
pub const LINK_HOVER: u32 = 0x1c64c8;
pub const TEXT_BLACK: u32 = 0x000000;
pub const TEXT_GREY: u32 = 0x6b6b6b;
pub const TEXT_LIGHT_GREY: u32 = 0x8a8a8a;
pub const BORDER_BLUE: u32 = 0x91a7c8;
pub const BORDER_SOFT: u32 = 0xc9d7ea;
pub const PANEL_BLUE: u32 = 0xe8f9ff;
pub const SELECT_BLUE: u32 = 0xbfe9fa;
pub const HEADER_TOP: u32 = 0xf2fdff;
pub const HEADER_BOTTOM: u32 = 0x93d9ef;

pub const WALLPAPERS: &[(&str, &str)] = &[
    ("classic", "Classic"),
    ("glass-dome", "Light: Glass island"),
    ("blue-water", "Light: Blue water"),
    ("green-city", "Light: Green city"),
    ("aero-valley", "Light: Aero valley"),
    ("winter-rabbit", "Light: Winter"),
    ("night-forest", "Night: Forest"),
    ("night-bridge", "Night: Bridge"),
    ("aurora", "Night: Aurora"),
];

static NIGHT: AtomicBool = AtomicBool::new(false);

pub fn is_night() -> bool {
    NIGHT.load(Ordering::Relaxed)
}

pub fn set_night(night: bool) {
    NIGHT.store(night, Ordering::Relaxed);
}

/// Only bundled identifiers are allowed; the collection is independent of the theme.
pub fn wallpaper_allowed(name: &str) -> bool {
    WALLPAPERS.iter().any(|(id, _)| *id == name)
}

/// Background used when the current choice does not fit the theme.
pub fn default_wallpaper() -> Option<&'static str> {
    if is_night() {
        Some("night-forest")
    } else {
        None
    }
}

pub fn vertical(top: u32, bottom: u32) -> Background {
    linear_gradient(
        180.0,
        linear_color_stop(rgb(top), 0.0),
        linear_color_stop(rgb(bottom), 1.0),
    )
}

pub fn title_gradient() -> Background {
    if is_night() {
        vertical(0x050d15, 0x0d2a3b)
    } else {
        vertical(TITLE_MID, TITLE_LOW)
    }
}

pub fn header_gradient() -> Background {
    if is_night() {
        vertical(0x071a26, 0x0d384b)
    } else {
        vertical(HEADER_TOP, HEADER_BOTTOM)
    }
}

pub fn login_gradient() -> Background {
    if is_night() {
        vertical(0x040a11, 0x0a2938)
    } else {
        vertical(0xf2fdff, 0x9fdff1)
    }
}

pub fn toolbar_gradient() -> Background {
    if is_night() {
        vertical(0x0a1e2a, 0x0d3142)
    } else {
        vertical(0xffffff, 0xbfeaf8)
    }
}

pub fn panel_gradient() -> Background {
    if is_night() {
        vertical(0x091c27, 0x0b2b3b)
    } else {
        vertical(0xf8feff, 0xb8e4f6)
    }
}

/// Glossy dome used for primary buttons.
pub fn aqua_button() -> Background {
    if is_night() {
        vertical(0x1b6479, 0x0d3244)
    } else {
        vertical(0xffffff, 0x87d5ee)
    }
}

pub fn aqua_button_hover() -> Background {
    if is_night() {
        vertical(0x2a8298, 0x124556)
    } else {
        vertical(0xffffff, 0xb9efff)
    }
}

pub fn window_bg() -> Hsla {
    if is_night() {
        rgb(0x06111a).into()
    } else {
        gpui::white().into()
    }
}

pub fn panel_bg() -> Hsla {
    if is_night() {
        rgb(0x0a1e2b).into()
    } else {
        gpui::white().into()
    }
}

/// Raised settings/request surfaces. Slightly lighter than the panel in Night.
pub fn card_bg() -> Hsla {
    if is_night() {
        rgb(0x0d2736).into()
    } else {
        rgb(0xffffff).into()
    }
}

/// Readability wash over a conversation wallpaper. The art stays visible while
/// body text keeps its contrast in both palettes.
pub fn chat_scrim() -> Hsla {
    if is_night() {
        rgba(0x06131c8f).into()
    } else {
        rgba(0xffffff96).into()
    }
}

pub fn system_pill_bg() -> Hsla {
    if is_night() {
        rgba(0x071a26c7).into()
    } else {
        rgba(0xffffffc7).into()
    }
}

pub fn field_bg() -> Hsla {
    if is_night() {
        rgb(0x081b27).into()
    } else {
        gpui::white().into()
    }
}

pub fn field_border() -> Hsla {
    if is_night() {
        rgb(0x2c667d).into()
    } else {
        rgb(0x7f9ac1).into()
    }
}

pub fn border_color() -> Hsla {
    if is_night() {
        rgb(0x1d4c5f).into()
    } else {
        rgb(0x91a7c8).into()
    }
}

pub fn soft_border() -> Hsla {
    if is_night() {
        rgb(0x173c4c).into()
    } else {
        rgb(0xd6dfef).into()
    }
}

pub fn text_primary() -> Hsla {
    if is_night() {
        rgb(0xe4f9ff).into()
    } else {
        rgb(0x111111).into()
    }
}

pub fn text_secondary() -> Hsla {
    if is_night() {
        rgb(0xb0ced9).into()
    } else {
        rgb(0x41556b).into()
    }
}

pub fn link_color() -> Hsla {
    if is_night() {
        rgb(0x7fe6ff).into()
    } else {
        rgb(LINK_BLUE).into()
    }
}

pub fn title_text() -> Hsla {
    if is_night() {
        rgb(0xdff8ff).into()
    } else {
        rgb(TITLE_TEXT).into()
    }
}

pub fn accent() -> Hsla {
    if is_night() {
        rgb(0x4fd6ff).into()
    } else {
        rgb(0x3fb6dd).into()
    }
}

pub fn button_text() -> Hsla {
    if is_night() {
        rgb(0xeaffff).into()
    } else {
        rgb(0x0a4a6b).into()
    }
}

pub fn selection_bg() -> Hsla {
    if is_night() {
        rgb(0x11465c).into()
    } else {
        rgb(SELECT_BLUE).into()
    }
}

pub fn row_hover() -> Hsla {
    if is_night() {
        rgb(0x0f3040).into()
    } else {
        rgb(0xeaf2ff).into()
    }
}

pub fn tile_bg() -> Hsla {
    if is_night() {
        rgba(0x7fe6ff18).into()
    } else {
        rgba(0xffffff66).into()
    }
}

pub fn danger() -> Hsla {
    if is_night() {
        rgb(0xff9a9a).into()
    } else {
        rgb(0x8a1f1f).into()
    }
}

pub fn notice_bg() -> Hsla {
    if is_night() {
        rgb(0x26210f).into()
    } else {
        rgb(0xfff8d4).into()
    }
}

pub fn notice_border() -> Hsla {
    if is_night() {
        rgb(0x4a4220).into()
    } else {
        rgb(0xe6dca6).into()
    }
}

pub fn notice_text() -> Hsla {
    if is_night() {
        rgb(0xffe9a3).into()
    } else {
        rgb(0x6a5a10).into()
    }
}

pub fn warning_bg() -> Hsla {
    if is_night() {
        rgb(0x2a1512).into()
    } else {
        rgb(0xffe9e4).into()
    }
}

pub fn warning_border() -> Hsla {
    if is_night() {
        rgb(0x5a2a22).into()
    } else {
        rgb(0xe3b4a6).into()
    }
}

pub fn list_bg() -> Background {
    if is_night() {
        vertical(0x06131c, 0x0a2532)
    } else {
        vertical(0xffffff, 0xf3fcff)
    }
}

pub fn chat_bg() -> Hsla {
    if is_night() {
        rgb(0x050f17).into()
    } else {
        gpui::white().into()
    }
}

pub fn chat_wallpaper_opacity() -> f32 {
    if is_night() { 0.18 } else { 0.30 }
}

pub fn bold() -> FontWeight {
    FontWeight::BOLD
}

pub fn status_color(status: crate::msnp::parsing::Status) -> Hsla {
    use crate::msnp::parsing::Status;
    if is_night() {
        match status {
            Status::Online => rgb(0x76e39a).into(),
            Status::Away | Status::Idle | Status::BeRightBack => rgb(0xffcf6e).into(),
            Status::Busy | Status::OnThePhone | Status::OutToLunch => rgb(0xff8f8f).into(),
            Status::Offline => rgb(0x7e9baa).into(),
        }
    } else {
        match status {
            Status::Online => rgb(0x2f7d2f).into(),
            Status::Away | Status::Idle | Status::BeRightBack => rgb(0xa06a00).into(),
            Status::Busy | Status::OnThePhone | Status::OutToLunch => rgb(0xa83232).into(),
            Status::Offline => rgb(0x777777).into(),
        }
    }
}

/// Stable per-contact colour used for the "Name says:" line, echoing the
/// hand-picked MSN nickname colours of the era.
pub fn nickname_color(email: &str) -> Hsla {
    const LIGHT: [u32; 10] = [
        0x9c1f1f, 0x1f5f9c, 0x6b2a8a, 0x1f7a5a, 0x9c5a1f, 0x2f4f8f, 0x8a2f6b, 0x4a6b1f, 0x8a4f1f,
        0x305f6b,
    ];
    const NIGHT: [u32; 10] = [
        0xff8f8f, 0x8fc8ff, 0xd7a6ff, 0x7fe8bb, 0xffc27a, 0xa9c5ff, 0xff9fd3, 0xd4f08a, 0xffb27a,
        0x8fe6f2,
    ];
    let hash = email.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    });
    let palette = if is_night() { NIGHT } else { LIGHT };
    rgb(palette[(hash as usize) % palette.len()]).into()
}
