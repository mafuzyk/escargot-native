use escargot_native::ui::theme;
use gpui::{Hsla, Rgba, rgb};

fn luminance(color: Hsla) -> f32 {
    let color = Rgba::from(color);
    let linear = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn assert_readable(label: &str, text: Hsla, surface: Hsla) {
    let a = luminance(text);
    let b = luminance(surface);
    let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
    assert!(
        ratio >= 4.5,
        "{label} needs 4.5:1 text contrast, got {ratio:.2}:1"
    );
}

#[test]
fn secondary_text_is_readable_on_every_main_surface_in_both_themes() {
    for night in [false, true] {
        theme::set_night(night);
        for surface in [
            theme::field_bg(),
            theme::panel_bg(),
            theme::selection_bg(),
            rgb(if night {
                0x0d384b
            } else {
                theme::HEADER_BOTTOM
            })
            .into(),
        ] {
            assert_readable("Secondary text", theme::text_secondary(), surface);
        }
    }
    theme::set_night(false);
}
