//! Built-in Escargot browser window.
//!
//! GPUI does not ship a web engine. This helper uses the same WebKitGTK engine
//! Tauri uses on Linux, in its own Aero-themed window, so links and search do
//! not have to leave the app for Firefox. If this binary is missing or fails to
//! start, the messenger falls back to the system browser.

use gtk::gdk;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, Entry, HeaderBar};
use webkit2gtk::{LoadEvent, WebView, WebViewExt};

const CSS: &str = r#"
window {
    background-image: linear-gradient(to bottom, #f4fdff, #a9ddf0);
}
headerbar {
    background-image: linear-gradient(to bottom, #ffffff, #bfe8f7);
    border-bottom: 2px solid #8cc6dc;
    box-shadow: inset 0 1px rgba(255,255,255,0.95);
    min-height: 40px;
    padding: 4px 6px;
}
headerbar button {
    background-image: linear-gradient(to bottom, #ffffff, #bfe8f7);
    border: 1px solid #7fb9d0;
    border-radius: 7px;
    color: #0a4a6b;
    padding: 2px 8px;
    box-shadow: inset 0 1px rgba(255,255,255,0.9);
}
headerbar button:hover {
    background-image: linear-gradient(to bottom, #ffffff, #dcf6ff);
}
entry {
    background: #ffffff;
    border: 1px solid #7fb9d0;
    border-radius: 8px;
    padding: 4px 9px;
    color: #12313f;
}
"#;

fn main() {
    // The URL travels through the environment because a GTK Application treats
    // bare argv entries as files to open and refuses them without HANDLES_OPEN.
    let url = std::env::var("ESCARGOT_WEBVIEW_URL")
        .unwrap_or_else(|_| "https://escargot.chat/".to_string());

    let app = Application::builder()
        .application_id("chat.escargot.Browser")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(CSS.as_bytes())
            .expect("webview stylesheet should parse");
        if let Some(screen) = gdk::Screen::default() {
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    app.connect_activate(move |app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Escargot Browser")
            .default_width(1024)
            .default_height(720)
            .build();

        let header = HeaderBar::new();
        header.set_show_close_button(true);

        let back = Button::from_icon_name(Some("go-previous-symbolic"), gtk::IconSize::Button);
        back.set_tooltip_text(Some("Back"));
        let forward = Button::from_icon_name(Some("go-next-symbolic"), gtk::IconSize::Button);
        forward.set_tooltip_text(Some("Forward"));
        let reload = Button::from_icon_name(Some("view-refresh-symbolic"), gtk::IconSize::Button);
        reload.set_tooltip_text(Some("Reload"));

        let entry = Entry::new();
        entry.set_width_chars(64);
        entry.set_text(&url);

        header.pack_start(&back);
        header.pack_start(&forward);
        header.pack_start(&reload);
        header.set_custom_title(Some(&entry));

        let view = WebView::new();
        view.load_uri(&url);

        let view_for_back = view.clone();
        back.connect_clicked(move |_| {
            if view_for_back.can_go_back() {
                view_for_back.go_back();
            }
        });
        let view_for_forward = view.clone();
        forward.connect_clicked(move |_| {
            if view_for_forward.can_go_forward() {
                view_for_forward.go_forward();
            }
        });
        let view_for_reload = view.clone();
        reload.connect_clicked(move |_| view_for_reload.reload());

        let view_for_entry = view.clone();
        entry.connect_activate(move |entry| {
            let mut target = entry.text().to_string();
            if !target.contains("://") {
                target = format!("https://{target}");
            }
            view_for_entry.load_uri(&target);
        });

        let entry_for_uri = entry.clone();
        view.connect_uri_notify(move |view| {
            if let Some(uri) = view.uri() {
                entry_for_uri.set_text(&uri);
            }
        });

        let window_for_title = window.clone();
        view.connect_title_notify(move |view| {
            if let Some(title) = view.title() {
                window_for_title.set_title(&format!("{title} - Escargot Browser"));
            }
        });

        let _ = LoadEvent::Finished;

        window.set_titlebar(Some(&header));
        window.add(&view);
        window.show_all();
    });

    app.run();
}
