//! Appearance settings must preserve existing profiles and keep surface choices independent.
use escargot_native::{
    config::Config,
    ui::{scale, theme},
};

#[test]
fn zoom_steps_stay_within_a_readable_and_recoverable_range() {
    assert_eq!(scale::adjusted_percent(100, 25), 125);
    assert_eq!(scale::adjusted_percent(100, -25), 75);
    assert_eq!(scale::adjusted_percent(75, -25), 75);
    assert_eq!(scale::adjusted_percent(200, 25), 200);
    scale::set_percent(125);
    assert_eq!(scale::px(16.), gpui::px(20.));
    scale::set_percent(100);
    assert_eq!(scale::px(16.), gpui::px(16.));
}

#[test]
fn existing_profiles_keep_their_chat_background_and_default_to_normal_zoom() {
    let settings: Config = toml::from_str("wallpaper = 'blue-water'\nnight = false\n").unwrap();
    assert_eq!(settings.wallpaper.as_deref(), Some("blue-water"));
    assert_eq!(settings.interface_wallpaper, None);
    assert_eq!(settings.ui_zoom, 100);
}

#[test]
fn interface_and_conversation_backgrounds_round_trip_independently() {
    let settings: Config = toml::from_str(
        "wallpaper = 'night-forest'\ninterface_wallpaper = 'blue-water'\nui_zoom = 150\n",
    )
    .unwrap();
    let restored: Config = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.wallpaper.as_deref(), Some("night-forest"));
    assert_eq!(restored.interface_wallpaper.as_deref(), Some("blue-water"));
    assert_eq!(restored.ui_zoom, 150);
}

#[test]
fn narrow_single_windows_switch_to_one_pane_instead_of_overlapping() {
    use escargot_native::ui::contacts::is_compact_layout;
    // 860 physical px at 200% is 430 design px: the 300 px sidebar plus a
    // usable transcript cannot fit, so only one pane may render.
    assert!(is_compact_layout(true, 860., 200));
    assert!(is_compact_layout(true, 860., 125));
    assert!(!is_compact_layout(true, 860., 100));
    assert!(is_compact_layout(true, 1000., 200));
    // Separate windows already show one view per window.
    assert!(!is_compact_layout(false, 360., 200));
}

#[test]
fn both_builtin_wallpaper_collections_are_available_in_either_theme() {
    for night in [false, true] {
        theme::set_night(night);
        for name in [
            "classic",
            "blue-water",
            "glass-dome",
            "green-city",
            "aero-valley",
            "winter-rabbit",
            "night-forest",
            "night-bridge",
            "aurora",
        ] {
            assert!(
                theme::wallpaper_allowed(name),
                "Both wallpaper collections must stay available: {name}"
            );
        }
        for invalid in [
            "missing-wallpaper",
            "../../private",
            "/etc/passwd",
            "night-forest.jpg",
        ] {
            assert!(
                !theme::wallpaper_allowed(invalid),
                "Only bundled wallpaper identifiers may be used"
            );
        }
    }
    theme::set_night(false);
}
