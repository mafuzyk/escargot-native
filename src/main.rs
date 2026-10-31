use escargot_native::config;
use escargot_native::state::Store;
use escargot_native::ui::{
    self, AddContact, CloseChat, NextTab, OpenSettings, PreviousTab, RefreshContacts, SendMessage,
    SignIn, SignOut, SubmitContact, add_contact, assets, browser,
    contacts,
    conversation::ConversationView,
    games, input, login,
};
use gpui::{
    App, prelude::*, Application, Bounds, KeyBinding, WindowBounds, WindowDecorations, WindowOptions, px, size,
};

fn main() {
    env_logger::init();

    Application::new().with_assets(assets::Assets).run(|cx: &mut App| {
        assets::load_fonts(cx);
        input::bind_input_keys(cx);
        ui::scale::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("enter", SendMessage, Some("MessageComposer")),
            KeyBinding::new("enter", SignIn, Some("LoginWindow")),
            KeyBinding::new("enter", SubmitContact, Some("AddContactInput")),
            KeyBinding::new("ctrl-n", AddContact, Some("ContactsWindow")),
            KeyBinding::new("f5", RefreshContacts, Some("ContactsWindow")),
            KeyBinding::new("ctrl-shift-q", SignOut, Some("ContactsWindow")),
            KeyBinding::new("ctrl-tab", NextTab, Some("ContactsWindow")),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, Some("ContactsWindow")),
            KeyBinding::new("ctrl-w", CloseChat, Some("ContactsWindow")),
            KeyBinding::new("ctrl-,", OpenSettings, Some("ContactsWindow")),
        ]);

        let store = cx.new(|cx| Store::new(cx));

        let login_probe = std::env::args().any(|arg| arg == "--login");
        if login_probe {
            let email = std::env::var("ESCARGOT_EMAIL").unwrap_or_default();
            let password = std::env::var("ESCARGOT_PASSWORD").unwrap_or_default();
            store.update(cx, |store, cx| {
                store.sign_in(&email, &password, cx);
                cx.notify();
            });
            contacts::open_contacts_window(store.clone(), cx);
            cx.activate(true);
            return;
        }

        if std::env::args().any(|arg| arg == "--demo-solitaire") {
            games::open_games_window(cx);
            cx.activate(true);
            return;
        }

        if std::env::args().any(|arg| arg == "--demo-checkers") {
            ui::checkers::open_checkers_window(cx);
            cx.activate(true);
            return;
        }

        if std::env::args().any(|arg| arg == "--demo-chess") {
            ui::chess::open_chess_window(cx);
            cx.activate(true);
            return;
        }

        if std::env::args().any(|arg| arg == "--demo-settings") {
            store.update(cx, |store, cx| {
                store.demo = true;
                store.sign_in("he@escargot.chat", "demo", cx);
                cx.notify();
            });
            contacts::open_contacts_window(store.clone(), cx);
            cx.activate(true);
            return;
        }

        let demo_add = std::env::args().any(|arg| arg == "--demo-add");
        let demo_browser = std::env::args().any(|arg| arg == "--demo-browser");
        let demo =
            std::env::args().any(|arg| arg == "--demo") || demo_add || demo_browser;
        if demo {
            store.update(cx, |store, cx| {
                store.demo = true;
                store.sign_in("he@escargot.chat", "demo", cx);
                cx.notify();
            });
            contacts::open_contacts_window(store.clone(), cx);

            if demo_browser {
                browser::open("https://escargot.chat/", cx);
                cx.activate(true);
                return;
            }

            if demo_add {
                add_contact::open_add_contact_window(store.clone(), String::new(), None, cx);
                cx.activate(true);
                return;
            }

            let chat_bounds = Bounds::centered(None, size(px(499.), px(472.)), cx);
            let store_for_chat = store.clone();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(chat_bounds)),
                    app_id: Some("escargot-native-chat".into()),
                    window_decorations: Some(WindowDecorations::Client),
                    is_resizable: true,
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| {
                        ConversationView::new(store_for_chat.clone(), "alex@escargot.chat".into(), cx)
                    })
                },
            )
            .ok();
            cx.activate(true);
            return;
        }

        let settings = config::load();
        if settings.auto_sign_in {
            if let Some(email) = settings.email.clone() {
                if let Some(password) = config::load_password(&email) {
                    store.update(cx, |store, cx| {
                        store.sign_in(&email, &password, cx);
                        cx.notify();
                    });
                    contacts::open_contacts_window(store.clone(), cx);
                    cx.activate(true);
                    return;
                }
            }
        }

        login::open_login_window(store.clone(), cx);

        cx.activate(true);
        let _ = ui::theme::FONT;
    });
}
