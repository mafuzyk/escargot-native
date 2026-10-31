use std::collections::{HashMap, HashSet};

use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, FocusHandle, Focusable,
    Render, ScrollHandle, Window, WindowBounds, WindowDecorations, WindowHandle, WindowOptions,
    div, img, prelude::*, size,
};
use crate::ui::scale::px;

use crate::msnp::parsing::Status;
use crate::state::{Contact, Store, elide, friendly_name};
use crate::ui::assets::avatar_for;
use crate::ui::conversation::ConversationView;
use crate::ui::input::TextInput;
use crate::ui::{
    AddContact, CloseChat, NextTab, OpenSettings, PreviousTab, RefreshContacts, SignOut,
};
use crate::ui::{browser, login, theme, widgets, wallpaper};

/// The split view needs room for the 300 px sidebar plus a usable transcript.
/// Below 720 design pixels, show one pane at a time; otherwise a narrow tiled
/// window (common on Hyprland) draws the chat over the contact list.
pub fn is_compact_layout(single_window: bool, physical_width: f32, zoom_percent: u16) -> bool {
    if !single_window {
        return false;
    }
    let zoom = zoom_percent.max(1) as f32 / 100.;
    physical_width / zoom < 720.
}

pub fn open_contacts_window(store: Entity<Store>, cx: &mut App) {    let single = store.read(cx).single_window;
    let (width, height) = if single { (1000., 720.) } else { (360., 640.) };
    let bounds = Bounds::centered(None, size(gpui::px(width), gpui::px(height)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("escargot-native-contacts".into()),
            window_decorations: Some(WindowDecorations::Client),
            is_resizable: true,
            window_min_size: Some(if single {
                size(gpui::px(760.), gpui::px(560.))
            } else {
                size(gpui::px(340.), gpui::px(480.))
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| ContactsView::new(store.clone(), cx)),
    );

    if let Ok(handle) = handle {
        handle
            .update(cx, |view, window, _| {
                window.focus(&view.focus);
                window.activate_window();
            })
            .ok();
    }
}

/// Stable focus handles for the settings controls. They let the panel scroll
/// the section that just received keyboard focus into view, which matters at
/// 150-200% zoom where settings are taller than the window.
struct SettingsFocus {
    done: FocusHandle,
    single: FocusHandle,
    separate: FocusHandle,
    light: FocusHandle,
    night: FocusHandle,
    smaller: FocusHandle,
    larger: FocusHandle,
    reset: FocusHandle,
    interface_choose: FocusHandle,
    interface_classic: FocusHandle,
    conversation_choose: FocusHandle,
    conversation_classic: FocusHandle,
}

impl SettingsFocus {
    fn new(cx: &mut Context<ContactsView>) -> Self {
        // Explicit tracked handles do not inherit `tab_stop`/`tab_index` from
        // the element, so mark them here or Tab skips the whole settings panel.
        let handle = || cx.focus_handle().tab_stop(true).tab_index(0);
        Self {
            done: handle(),
            single: handle(),
            separate: handle(),
            light: handle(),
            night: handle(),
            smaller: handle(),
            larger: handle(),
            reset: handle(),
            interface_choose: handle(),
            interface_classic: handle(),
            conversation_choose: handle(),
            conversation_classic: handle(),
        }
    }

    /// 0 = title/conversations, 1 = theme, 2 = size and interface wallpaper,
    /// 3 = conversation wallpaper.
    fn section(&self, focused: &FocusHandle) -> Option<usize> {
        if focused == &self.done || focused == &self.single || focused == &self.separate {
            Some(0)
        } else if focused == &self.light || focused == &self.night {
            Some(1)
        } else if focused == &self.smaller
            || focused == &self.larger
            || focused == &self.reset
            || focused == &self.interface_choose
            || focused == &self.interface_classic
        {
            Some(2)
        } else if focused == &self.conversation_choose || focused == &self.conversation_classic {
            Some(3)
        } else {
            None
        }
    }
}

pub struct ContactsView {
    store: Entity<Store>,
    windows: HashMap<String, WindowHandle<ConversationView>>,
    embedded: HashMap<String, Entity<ConversationView>>,
    tabs: Vec<String>,
    active: Option<String>,
    focus: FocusHandle,
    settings_focus: SettingsFocus,
    settings_scroll: ScrollHandle,
    settings_section: Option<usize>,
    pending_demo_chat: Option<String>,
    settings_open: bool,
    menu_open: bool,
    add_form_open: bool,
    focus_add_input: bool,
    wallpaper_picker: Option<wallpaper::Target>,
    search: Entity<TextInput>,
    contact_search: Entity<TextInput>,
    add_input: Entity<TextInput>,
}

impl ContactsView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let search = cx.new(|cx| TextInput::new("Search the web", false, cx));
        let contact_search = cx.new(|cx| TextInput::new("Search contacts", false, cx));
        let add_input = cx.new(|cx| TextInput::new("username or email", false, cx));
        cx.observe(&contact_search, |_, _, cx| cx.notify()).detach();
        cx.observe(&add_input, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            windows: HashMap::new(),
            embedded: HashMap::new(),
            tabs: Vec::new(),
            active: None,
            focus: cx.focus_handle(),
            settings_focus: SettingsFocus::new(cx),
            settings_scroll: ScrollHandle::new(),
            settings_section: None,
            pending_demo_chat: std::env::var("ESCARGOT_DEMO_CHAT").ok(),
            settings_open: std::env::var_os("ESCARGOT_SETTINGS").is_some(),
            menu_open: false,
            add_form_open: std::env::var_os("ESCARGOT_ADD_FORM").is_some(),
            focus_add_input: std::env::var_os("ESCARGOT_ADD_FORM").is_some(),
            wallpaper_picker: None,
            search,
            contact_search,
            add_input,
        }
    }

    /// Opens a conversation and gives the composer focus in single-window mode.
    fn open_contact(&mut self, email: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_chat(email.clone(), cx);
        if let Some(view) = self.embedded.get(&email) {
            view.update(cx, |view, cx| view.focus_input(window, cx));
        }
        cx.notify();
    }

    fn open_chat(&mut self, email: String, cx: &mut Context<Self>) {
        if self.store.read(cx).single_window {
            if !self.embedded.contains_key(&email) {
                let store = self.store.clone();
                let peer = email.clone();
                let view = cx.new(|cx| ConversationView::new_embedded(store, peer, cx));
                self.embedded.insert(email.clone(), view);
                self.tabs.push(email.clone());
            }
            self.active = Some(email.clone());
            self.settings_open = false;
            self.store.update(cx, |store, cx| {
                store.open_chat(&email);
                cx.notify();
            });
            cx.notify();
            return;
        }

        if let Some(handle) = self.windows.get(&email).cloned() {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                self.store.update(cx, |store, cx| {
                    store.open_chat(&email);
                    cx.notify();
                });
                return;
            }
            self.windows.remove(&email);
        }

        let store = self.store.clone();
        let email_for_window = email.clone();
        let bounds = Bounds::centered(None, size(gpui::px(499.), gpui::px(472.)), cx);
        let handle = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("escargot-native-chat".into()),
                window_decorations: Some(WindowDecorations::Client),
                is_resizable: true,
                window_min_size: Some(size(gpui::px(420.), gpui::px(360.))),
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|cx| ConversationView::new(store.clone(), email_for_window.clone(), cx))
            },
        );

        if let Ok(handle) = handle {
            handle
                .update(cx, |_, window, _| window.activate_window())
                .ok();
            self.windows.insert(email.clone(), handle);
            self.store.update(cx, |store, cx| {
                store.open_chat(&email);
                cx.notify();
            });
        }
    }

    fn toggle_group(&mut self, name: String, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            if let Some(group) = store.groups.iter_mut().find(|group| group.name == name) {
                group.open = !group.open;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn choose_status(&mut self, status: Status, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.set_status(status);
            cx.notify();
        });
        self.menu_open = false;
        cx.notify();
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let status = if store.connecting {
            "Signing in…"
        } else if !store.signed_in {
            "Offline"
        } else {
            store.status.label()
        };
        let name = store.own_label();
        let email = store.email.clone();
        let online = store.signed_in && store.status != Status::Offline;
        div()
            .h(px(84.))
            .flex_shrink_0()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px(px(16.))
            .py(px(10.))
            .bg(theme::header_gradient())
            .border_b_1()
            .border_color(theme::border_color())
            .child(
                div()
                    .relative()
                    .size(px(56.))
                    .flex_shrink_0()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::soft_border())
                    .bg(theme::field_bg())
                    .p(px(2.))
                    .child(
                        img("assets/img/avatar_default.png")
                            .size_full()
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom(px(0.))
                            .right(px(0.))
                            .rounded_full()
                            .bg(theme::field_bg())
                            .p(px(1.))
                            .child(widgets::status_icon_sized(
                                if online { store.status } else { Status::Offline },
                                12.,
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(theme::bold())
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                widgets::button(format!("{status} ▾"))
                                    .h(px(28.))
                                    .px(px(9.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.menu_open = !this.menu_open;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .truncate()
                            .child(email),
                    ),
            )
            .child(
                widgets::button("Settings")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
            )
    }

    fn contact_row(
        &self,
        contact: Contact,
        unread: usize,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let email = contact.email.clone();
        let click_email = email.clone();
        let key_email = email.clone();
        let pending = contact.pending_outgoing;
        let detail = if pending {
            "Waiting for them to add you".to_string()
        } else {
            contact
                .psm
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| {
                    if contact.status == Status::Offline {
                        "Offline"
                    } else {
                        contact.status.label()
                    }
                })
                .to_string()
        };
        let label = elide(&contact.label(), 26);
        let detail = elide(&detail, 34);
        div()
            .id(gpui::SharedString::from(contact.email.clone()))
            .focusable()
            .tab_index(0)
            .min_h(px(54.))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .px(px(12.))
            .py(px(7.))
            .border_1()
            .border_color(gpui::rgba(0))
            .bg(if selected {
                theme::selection_bg()
            } else {
                gpui::rgba(0).into()
            })
            .cursor_pointer()
            .hover(|style| style.bg(theme::row_hover()))
            .focus(|style| style.border_color(theme::link_color()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_contact(click_email.clone(), window, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.open_contact(key_email.clone(), window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .relative()
                    .size(px(32.))
                    .flex_shrink_0()
                    .child(
                        img(avatar_for(&contact.email))
                            .size_full()
                            .rounded_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom(px(0.))
                            .right(px(0.))
                            .rounded_full()
                            .bg(theme::field_bg())
                            .p(px(1.))
                            .child(widgets::status_icon_sized(contact.status, 11.)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .max_w(gpui::relative(0.72))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(if unread > 0 || pending {
                                theme::bold()
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .truncate()
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(if pending {
                                theme::link_color()
                            } else {
                                theme::text_secondary()
                            })
                            .truncate()
                            .child(detail),
                    ),
            )
            .children(pending.then(|| {
                div()
                    .px(px(6.))
                    .py(px(1.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::accent())
                    .text_size(px(10.))
                    .text_color(theme::link_color())
                    .flex_shrink_0()
                    .child("Pending")
            }))
            .children((unread > 0).then(|| {
                div()
                    .min_w(px(20.))
                    .h(px(20.))
                    .px(px(5.))
                    .rounded_full()
                    .bg(theme::selection_bg())
                    .text_size(px(11.))
                    .text_color(theme::text_primary())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(unread.to_string())
            }))
            .into_any_element()
    }

    fn add_contact_action(&mut self, _: &AddContact, window: &mut Window, cx: &mut Context<Self>) {
        self.open_add_form(window, cx);
    }

    fn open_add_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let initial = self.contact_search.read(cx).content().trim().to_string();
        if !self.add_form_open {
            self.add_form_open = true;
            self.add_input
                .update(cx, |input, cx| input.set_text(initial, cx));
        }
        self.focus_add_input = true;
        self.store.update(cx, |store, cx| {
            store.prepare_add_form();
            cx.notify();
        });
        window.focus(&self.add_input.focus_handle(cx));
        cx.notify();
    }

    fn close_add_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.read(cx).adding_contact.is_some() {
            return;
        }
        self.add_form_open = false;
        self.add_input.update(cx, |input, cx| input.clear(cx));
        self.store.update(cx, |store, cx| {
            store.prepare_add_form();
            cx.notify();
        });
        window.focus(&self.contact_search.focus_handle(cx));
        cx.notify();
    }

    fn submit_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let account = self.add_input.read(cx).content();
        self.store.update(cx, |store, cx| {
            store.add_contact(&account, cx);
            cx.notify();
        });
        if self
            .store
            .read(cx)
            .contact_add_result
            .as_ref()
            .is_some_and(|result| result.is_err())
        {
            window.focus(&self.add_input.focus_handle(cx));
        }
        cx.notify();
    }

    fn refresh_contacts(&mut self, _: &RefreshContacts, _: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.refresh(cx);
            cx.notify();
        });
    }

    fn toggle_night(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.toggle_night(cx);
            cx.notify();
        });
        self.menu_open = false;
        cx.notify();
    }

    fn refresh_from_menu(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.refresh(cx);
            cx.notify();
        });
        self.menu_open = false;
        cx.notify();
    }

    fn sign_out_action(&mut self, _: &SignOut, window: &mut Window, cx: &mut Context<Self>) {
        self.perform_sign_out(window, cx);
    }

    fn sign_out(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.perform_sign_out(window, cx);
    }

    fn perform_sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.sign_out();
            cx.notify();
        });
        let windows: Vec<_> = self.windows.drain().map(|(_, handle)| handle).collect();
        for handle in windows {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        }
        login::open_login_window(store, cx);
        window.remove_window();
    }

    fn open_web_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).content();
        let query = query.trim();
        if !query.is_empty() {
            browser::open(
                &format!("https://duckduckgo.com/?q={}", urlencoding::encode(query)),
                cx,
            );
        }
    }

    fn connection_banner(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let store = self.store.read(cx);
        let connecting = store.connecting;
        let refreshing = store.refreshing;
        let error = store.connection_error.clone();
        let can_retry = store.can_retry_connection();

        if let Some(error) = error {
            vec![
                div()
                    .min_h(px(40.))
                    .flex_shrink_0()
                    .py(px(8.))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(10.))
                    .bg(theme::warning_bg())
                    .border_b_1()
                    .border_color(theme::warning_border())
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .font_family(theme::FONT)
                            .text_size(px(12.))
                            .text_color(theme::danger())
                            .child(error),
                    )
                    .children(can_retry.then(|| {
                        widgets::text_button("Retry")
                            .text_color(theme::danger())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.store.update(cx, |store, cx| {
                                    store.retry_connection(cx);
                                    cx.notify();
                                });
                            }))
                    }))
                    .child(
                        widgets::text_button("Sign in again")
                            .text_color(theme::danger())
                            .on_click(cx.listener(|this, _, window, cx| {
                                login::open_login_window(this.store.clone(), cx);
                                window.remove_window();
                            })),
                    )
                    .into_any_element(),
            ]
        } else if connecting {
            vec![
                div()
                    .min_h(px(36.))
                    .flex_shrink_0()
                    .py(px(7.))
                    .w_full()
                    .flex()
                    .items_center()
                    .px(px(10.))
                    .bg(theme::header_gradient())
                    .border_b_1()
                    .border_color(theme::soft_border())
                    .font_family(theme::FONT)
                    .text_size(px(12.))
                    .text_color(theme::button_text())
                    .child(if refreshing {
                        "Syncing contacts with Escargot…"
                    } else {
                        "Connecting to Escargot…"
                    })
                    .into_any_element(),
            ]
        } else {
            Vec::new()
        }
    }

    fn status_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.store.read(cx).status;
        let choices = [
            Status::Online,
            Status::Busy,
            Status::Away,
            Status::BeRightBack,
            Status::Idle,
            Status::OnThePhone,
            Status::OutToLunch,
            Status::Offline,
        ];

        div()
            .absolute()
            .top(px(112.))
            .left(px(88.))
            .w(px(208.))
            .rounded_md()
            .bg(theme::field_bg())
            .border_1()
            .border_color(theme::border_color())
            .shadow_lg()
            .py(px(3.))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu_open = false;
                cx.notify();
            }))
            .children(choices.into_iter().map(|status| {
                div()
                    .id(status.label())
                    .focusable()
                    .tab_index(0)
                    .focus(|style| style.bg(theme::selection_bg()))
                    .h(px(32.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .bg(if status == current {
                        theme::selection_bg()
                    } else {
                        theme::field_bg()
                    })
                    .hover(|style| style.bg(theme::row_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_status(status, cx)))
                    .child(widgets::status_icon(status))
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(11.))
                            .text_color(theme::text_primary())
                            .child(status.label()),
                    )
            }))
            .child(div().h(px(1.)).w_full().bg(theme::soft_border()))
            .child(
                div()
                    .id("theme-menu")
                    .focusable()
                    .tab_index(0)
                    .focus(|style| style.border_color(theme::link_color()).border_1())
                    .h(px(32.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::row_hover()))
                    .on_click(cx.listener(Self::toggle_night))
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(11.))
                            .text_color(theme::text_primary())
                            .child("Frutiger Night"),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(11.))
                            .text_color(theme::accent())
                            .child(if self.store.read(cx).night { "✓" } else { "" }),
                    ),
            )
            .child(
                div()
                    .id("refresh-menu")
                    .focusable()
                    .tab_index(0)
                    .focus(|style| style.border_color(theme::link_color()).border_1())
                    .h(px(32.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::row_hover()))
                    .on_click(cx.listener(Self::refresh_from_menu))
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(11.))
                            .text_color(theme::text_primary())
                            .child("Refresh Contacts"),
                    ),
            )
            .child(
                div()
                    .id("sign-out-menu")
                    .focusable()
                    .tab_index(0)
                    .focus(|style| style.border_color(theme::link_color()).border_1())
                    .h(px(32.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::warning_bg()))
                    .on_click(cx.listener(Self::sign_out))
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(11.))
                            .text_color(theme::danger())
                            .child("Sign Out"),
                    ),
            )
    }

    fn activate_tab(&mut self, email: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        if let Some(view) = email.as_ref().and_then(|email| self.embedded.get(email)) {
            view.update(cx, |view, cx| view.focus_input(window, cx));
        }
        self.settings_open = false;
        self.active = email.clone();
        self.store.update(cx, |store, cx| {
            if let Some(email) = &email {
                store.open_chat(email);
            }
            store.set_active_chat(email, cx);
            cx.notify();
        });
        cx.notify();
    }

    fn close_tab(&mut self, email: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.embedded.remove(email);
        self.tabs.retain(|tab| tab != email);
        if self.active.as_deref() == Some(email) {
            self.active = None;
        }
        self.store.update(cx, |store, cx| {
            store.close_chat(email, cx);
        });
        cx.notify();
    }

    fn choose_single(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.set_single_window(true, cx);
            store.set_active_chat(None, cx);
            cx.notify();
        });
        window.resize(size(gpui::px(1000.), gpui::px(720.)));
        cx.notify();
    }

    fn choose_separate(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.set_single_window(false, cx);
            store.set_active_chat(None, cx);
            cx.notify();
        });
        for email in self.tabs.clone() {
            self.store.update(cx, |store, cx| {
                store.close_chat(&email, cx);
            });
        }
        self.active = None;
        window.resize(size(gpui::px(360.), gpui::px(640.)));
        cx.notify();
    }

    fn choose_light(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            if store.night {
                store.toggle_night(cx);
            }
            cx.notify();
        });
        cx.notify();
    }

    fn choose_night(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            if !store.night {
                store.toggle_night(cx);
            }
            cx.notify();
        });
        cx.notify();
    }

    fn close_settings(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = false;
        cx.notify();
    }

    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(1, window, cx);
    }

    fn previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(-1, window, cx);
    }

    fn cycle_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.store.read(cx).single_window {
            return;
        }
        let mut order: Vec<Option<String>> = vec![None];
        order.extend(self.tabs.iter().cloned().map(Some));
        let current = order
            .iter()
            .position(|entry| entry.as_deref() == self.active.as_deref())
            .unwrap_or(0) as isize;
        let next = (current + step).rem_euclid(order.len() as isize) as usize;
        let email = order[next].clone();
        self.activate_tab(email, window, cx);
    }

    fn close_chat_action(&mut self, _: &CloseChat, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(email) = self.active.clone() {
            self.close_tab(&email, window, cx);
        }
    }

    fn settings_action(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        // Start keyboard navigation at the first settings control so Tab order
        // does not depend on what was focused in the chat behind it.
        window.focus(&self.settings_focus.done);
        self.settings_section = Some(0);
        self.settings_open = true;
        self.active = None;
        self.store.update(cx, |store, cx| {
            store.set_active_chat(None, cx);
        });
        cx.notify();
    }

    fn shell(&self, content: AnyElement, cx: &mut Context<Self>) -> Div {
        div()
            .key_context("ContactsWindow")
            .track_focus(&self.focus)
            .tab_stop(false)
            .on_action(cx.listener(Self::add_contact_action))
            .on_action(cx.listener(Self::refresh_contacts))
            .on_action(cx.listener(Self::sign_out_action))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::close_chat_action))
            .on_action(cx.listener(Self::settings_action))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::field_bg())
            .font_family(theme::FONT)
            .text_size(px(13.))
            .text_color(theme::text_primary())
            .border_1()
            .border_color(theme::border_color())
            .on_key_down(widgets::navigate_focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.menu_open = false;
                    if this.settings_open && this.store.read(cx).adding_contact.is_none() {
                        this.settings_open = false;
                    }
                    if this.add_form_open && this.store.read(cx).adding_contact.is_none() {
                        this.add_form_open = false;
                    }
                    window.focus(&this.focus);
                    cx.notify();
                }
                if event.keystroke.modifiers.control && event.keystroke.key == "f" {
                    window.focus(&this.contact_search.focus_handle(cx));
                    cx.stop_propagation();
                }
            }))
            .children(wallpaper::layer(self.store.read(cx).interface_wallpaper.as_deref()))
            .child(widgets::title_bar(
                "MSN Messenger",
                |_, window, _| window.minimize_window(),
                |_, window, _| window.toggle_fullscreen(),
                |_, window, _| window.remove_window(),
            ))
            .child(self.header(cx))
            .children(self.connection_banner(cx))
            .child(self.tabs(cx))
            .child(content)
            .children(if self.menu_open {
                Some(self.status_menu(cx))
            } else {
                None
            })
    }

    fn tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let single = self.store.read(cx).single_window;
        if !single {
            return div().h(px(0.)).into_any_element();
        }
        let active = self.active.clone();
        let names: Vec<(String, String)> = self
            .tabs
            .iter()
            .map(|email| {
                let store = self.store.read(cx);
                let name = store.contact_name(email);
                let unread = store.unread_for(email);
                let name = if unread > 0 {
                    format!("{name} ({unread})")
                } else {
                    name
                };
                (email.clone(), name)
            })
            .collect();

        div()
            .id("chat-tabs")
            .h(px(36.))
            .flex_shrink_0()
            .w_full()
            .overflow_x_scroll()
            .flex()
            .items_center()
            .bg(theme::toolbar_gradient())
            .border_b_1()
            .border_color(theme::soft_border())
            .children(if single {
                Some(
                    div()
                        .h_full()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(self.tab("Contacts", None, active.is_none(), cx))
                        .children(names.into_iter().map(|(email, label)| {
                            let selected = active.as_deref() == Some(email.as_str());
                            self.tab(&label, Some(email), selected, cx)
                        }))
                        .child(div().flex_1())
                        .into_any_element(),
                )
            } else {
                None
            })
            .into_any_element()
    }

    fn tab(
        &self,
        label: &str,
        email: Option<String>,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let for_click = email.clone();
        let shown = if label.chars().count() > 18 {
            format!("{}…", label.chars().take(17).collect::<String>())
        } else {
            label.to_string()
        };
        let mut tab = div()
            .flex_shrink_0()
            .relative()
            .h_full()
            .flex()
            .items_center()
            .gap_1()
            .border_r_1()
            .border_color(theme::soft_border())
            .bg(if selected {
                theme::field_bg().into()
            } else {
                theme::toolbar_gradient()
            })
            .hover(|style| style.bg(theme::row_hover()))
            .child(
                div()
                    .id(gpui::SharedString::from(format!("tab-hit-{label}")))
                    .h_full()
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_tab(for_click.clone(), window, cx)
                    }))
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(12.))
                            .text_color(if selected {
                                theme::text_primary()
                            } else {
                                theme::text_secondary()
                            })
                            .child(shown),
                    ),
            );

        if selected {
            tab = tab.child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .right(px(0.))
                    .h(px(2.))
                    .bg(theme::accent()),
            );
        }

        if let Some(email) = email {
            let close_id = gpui::SharedString::from(format!("tab-close-{email}"));
            tab = tab.child(
                div()
                    .id(close_id)
                    .w(px(22.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .text_color(theme::text_secondary())
                    .cursor_pointer()
                    .hover(|style| style.text_color(theme::danger()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_tab(&email, window, cx);
                    }))
                    .child("×"),
            );
        }
        tab
    }

    /// Keeps the focused settings card visible when keyboard navigation moves
    /// past the fold. Tables are approximate because text wraps differently at
    /// each zoom level; the goal is to keep the focused control on screen.
    fn scroll_settings_for_focus(&mut self, window: &Window, cx: &App) {
        if !self.settings_open {
            self.settings_section = None;
            return;
        }
        let Some(focused) = window.focused(cx) else {
            return;
        };
        let Some(section) = self.settings_focus.section(&focused) else {
            return;
        };
        if self.settings_section == Some(section) {
            return;
        }
        self.settings_section = Some(section);
        let target = match section {
            0 => px(0.),
            1 => px(215.),
            2 => px(430.),
            _ => px(650.),
        };
        // `max_offset` is the positive scrollable distance; offsets are
        // negative. Clamp the target into that range before applying it.
        let limit = self.settings_scroll.max_offset().height;
        let target = target.max(px(0.)).min(limit);
        self.settings_scroll
            .set_offset(gpui::Point::new(px(0.), -target));
    }

    fn settings_panel(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let single = self.store.read(cx).single_window;
        let night = self.store.read(cx).night;

        div()
            .flex_1()
            .min_h(px(0.))
            .w_full()
            .id("settings-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .bg(if self.store.read(cx).interface_wallpaper.is_some() {
                wallpaper::wash(0.95).into()
            } else {
                theme::panel_gradient()
            })
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .p(px(20.))
                    .child(
                        div()
                            .w(px(720.))
                            .max_w(gpui::relative(0.94))
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(22.))
                                                    .font_weight(theme::bold())
                                                    .child("Settings"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(theme::text_secondary())
                                                    .child("Appearance and behavior for this account. Changes save automatically."),
                                            ),
                                    )
                                    .child(
                                        widgets::button("Done")
                                            .track_focus(&self.settings_focus.done)
                                            .on_click(cx.listener(Self::close_settings)),
                                    ),
                            )
                            .child(self.settings_card(
                                "Conversations",
                                "Choose how new chats open.",
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(self.choice_row(
                                        "single-window",
                                        "Single window with tabs",
                                        "Contacts stay beside the active chat.",
                                        single,
                                        &self.settings_focus.single,
                                        cx,
                                        Self::choose_single,
                                    ))
                                    .child(self.choice_row(
                                        "separate-windows",
                                        "Separate windows",
                                        "Each conversation gets its own window.",
                                        !single,
                                        &self.settings_focus.separate,
                                        cx,
                                        Self::choose_separate,
                                    ))
                                    .into_any_element(),
                            ))
                            .child(self.settings_card(
                                "Theme",
                                "Frutiger Aero for daylight, Frutiger Night for late sessions.",
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(self.theme_choice(
                                        "light",
                                        "Frutiger Aero",
                                        "Light",
                                        !night,
                                        &self.settings_focus.light,
                                        cx,
                                        Self::choose_light,
                                    ))
                                    .child(self.theme_choice(
                                        "night",
                                        "Frutiger Night",
                                        "Dark",
                                        night,
                                        &self.settings_focus.night,
                                        cx,
                                        Self::choose_night,
                                    ))
                                    .into_any_element(),
                            ))
                            .child(self.settings_card(
                                "Interface size",
                                "Ctrl++ and Ctrl+- resize every window. Ctrl+0 returns to 100%.",
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                widgets::button("Smaller")
                                                    .track_focus(&self.settings_focus.smaller)
                                                    .on_click(|_, window, cx| {
                                                    window.dispatch_action(
                                                        Box::new(crate::ui::scale::ZoomOut),
                                                        cx,
                                                    )
                                                }),
                                            )
                                            .child(
                                                div()
                                                    .min_w(px(64.))
                                                    .text_size(px(16.))
                                                    .font_weight(theme::bold())
                                                    .flex()
                                                    .justify_center()
                                                    .child(format!("{}%", crate::ui::scale::percent())),
                                            )
                                            .child(
                                                widgets::button("Larger")
                                                    .track_focus(&self.settings_focus.larger)
                                                    .on_click(|_, window, cx| {
                                                    window.dispatch_action(
                                                        Box::new(crate::ui::scale::ZoomIn),
                                                        cx,
                                                    )
                                                }),
                                            )
                                            .child(
                                                widgets::text_button("Reset")
                                                    .track_focus(&self.settings_focus.reset)
                                                    .on_click(|_, window, cx| {
                                                    window.dispatch_action(
                                                        Box::new(crate::ui::scale::ResetZoom),
                                                        cx,
                                                    )
                                                }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_secondary())
                                            .child("75% to 200%. Dialogs and games follow the same scale."),
                                    )
                                    .into_any_element(),
                            ))
                            .child(self.background_setting(wallpaper::Target::Interface, cx))
                            .child(self.background_setting(wallpaper::Target::Conversation, cx))
                            .child(
                                div()
                                    .pt(px(4.))
                                    .text_size(px(11.))
                                    .text_color(theme::text_secondary())
                                    .child("Wallpapers are bundled with the app; no external files are read."),
                            ),
                    ),
            )
    }

    fn settings_card(
        &self,
        title: &'static str,
        description: &'static str,
        body: AnyElement,
    ) -> Div {
        div()
            .p(px(16.))
            .rounded_lg()
            .bg(theme::card_bg())
            .border_1()
            .border_color(theme::soft_border())
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(theme::bold())
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .child(description),
                    ),
            )
            .child(body)
    }

    fn choice_row(
        &self,
        id: &'static str,
        label: &'static str,
        detail: &'static str,
        selected: bool,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .focusable()
            .track_focus(focus)
            .tab_index(0)
            .focus(|style| style.border_color(theme::link_color()))
            .min_h(px(44.))
            .w_full()
            .px(px(10.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap_3()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme::accent()
            } else {
                theme::soft_border()
            })
            .bg(if selected {
                theme::selection_bg()
            } else {
                gpui::rgba(0).into()
            })
            .cursor_pointer()
            .hover(|style| style.bg(theme::row_hover()))
            .on_click(cx.listener(handler))
            .child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        theme::accent()
                    } else {
                        theme::field_border()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(selected.then(|| {
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(theme::accent())
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(theme::bold())
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_secondary())
                            .child(detail),
                    ),
            )
    }

    fn theme_choice(
        &self,
        id: &'static str,
        label: &'static str,
        detail: &'static str,
        selected: bool,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
    ) -> gpui::Stateful<Div> {
        let (top, bottom, accent) = if id == "light" {
            (0xf2fdff, 0x9fdff1, 0x1c3f7c)
        } else {
            (0x050d15, 0x0d384b, 0x7fe6ff)
        };
        div()
            .id(id)
            .focusable()
            .track_focus(focus)
            .tab_index(0)
            .focus(|style| style.border_color(theme::link_color()))
            .flex_1()
            .min_w(px(220.))
            .p(px(10.))
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme::accent()
            } else {
                theme::soft_border()
            })
            .bg(if selected {
                theme::selection_bg()
            } else {
                theme::field_bg()
            })
            .flex()
            .flex_col()
            .gap_2()
            .cursor_pointer()
            .hover(|style| style.bg(theme::row_hover()))
            .on_click(cx.listener(handler))
            .child(
                div()
                    .h(px(42.))
                    .w_full()
                    .rounded_sm()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(div().flex_1().w_full().bg(gpui::rgb(top)))
                    .child(div().flex_1().w_full().bg(gpui::rgb(bottom)))
                    .child(div().h(px(4.)).w_full().bg(gpui::rgb(accent))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .font_weight(theme::bold())
                            .child(label),
                    )
                    .child(
                        div()
                            .size(px(14.))
                            .rounded_full()
                            .border_2()
                            .border_color(if selected {
                                theme::accent()
                            } else {
                                theme::field_border()
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .children(selected.then(|| {
                                div()
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(theme::accent())
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::text_secondary())
                    .child(detail),
            )
    }

    fn background_setting(&self, target: wallpaper::Target, cx: &mut Context<Self>) -> Div {
        let store = self.store.read(cx);
        let current = match target {
            wallpaper::Target::Interface => store.interface_wallpaper.clone(),
            wallpaper::Target::Conversation => store.wallpaper.clone(),
        };
        let label = theme::WALLPAPERS
            .iter()
            .find(|(id, _)| Some(*id) == current.as_deref())
            .map(|(_, label)| *label)
            .unwrap_or("Classic");
        let description = match target {
            wallpaper::Target::Interface => "Behind the contact list, header and settings.",
            wallpaper::Target::Conversation => "Behind the message transcript and composer.",
        };
        let preview = wallpaper::thumbnail(current.as_deref());
        let for_button = target;
        let (choose_focus, classic_focus) = match target {
            wallpaper::Target::Interface => (
                &self.settings_focus.interface_choose,
                &self.settings_focus.interface_classic,
            ),
            wallpaper::Target::Conversation => (
                &self.settings_focus.conversation_choose,
                &self.settings_focus.conversation_classic,
            ),
        };
        div()
            .p(px(16.))
            .rounded_lg()
            .bg(theme::card_bg())
            .border_1()
            .border_color(theme::soft_border())
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(theme::bold())
                            .child(target.title()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .child(description),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        img(preview)
                            .w(px(160.))
                            .h(px(90.))
                            .rounded_md()
                            .object_fit(gpui::ObjectFit::Cover)
                            .border_1()
                            .border_color(theme::soft_border()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(180.))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(theme::bold())
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_secondary())
                                    .child("Choose from the bundled Light and Night collections."),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        widgets::button(if self.wallpaper_picker == Some(target) {
                                            "Hide choices"
                                        } else {
                                            "Choose…"
                                        })
                                        .track_focus(choose_focus)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.wallpaper_picker = if this.wallpaper_picker
                                                == Some(for_button)
                                            {
                                                None
                                            } else {
                                                Some(for_button)
                                            };
                                            if this.wallpaper_picker.is_some() {
                                                // The gallery is not laid out yet, so the
                                                // scroll limit from this frame is too
                                                // small. Reveal it after the next layout.
                                                let scroll = this.settings_scroll.clone();
                                                window.on_next_frame(move |_, _| {
                                                    let limit = scroll.max_offset().height;
                                                    if limit > px(0.) {
                                                        let target =
                                                            (-scroll.offset().y + px(190.))
                                                                .min(limit);
                                                        scroll.set_offset(gpui::Point::new(
                                                            px(0.),
                                                            -target,
                                                        ));
                                                    }
                                                });
                                            }
                                            cx.notify();
                                        })),
                                    )
                                    .child(
                                        widgets::text_button("Use Classic")
                                            .track_focus(classic_focus)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.store.update(cx, |store, cx| match for_button {
                                                    wallpaper::Target::Interface => store
                                                        .set_interface_wallpaper(None, cx),
                                                    wallpaper::Target::Conversation => {
                                                        store.set_wallpaper(None, cx)
                                                    }
                                                });
                                                this.wallpaper_picker = None;
                                                let handle = match for_button {
                                                    wallpaper::Target::Interface => {
                                                        &this.settings_focus.interface_choose
                                                    }
                                                    wallpaper::Target::Conversation => {
                                                        &this.settings_focus.conversation_choose
                                                    }
                                                };
                                                window.focus(handle);
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            )
            .children((self.wallpaper_picker == Some(target)).then(|| {
                div()
                    .pt(px(10.))
                    .border_t_1()
                    .border_color(theme::soft_border())
                    .child(wallpaper::choices(
                        target.id(),
                        current,
                        cx.listener(move |this, choice: &Option<String>, window, cx| {
                            this.store.update(cx, |store, cx| match target {
                                wallpaper::Target::Interface => {
                                    store.set_interface_wallpaper(choice.clone(), cx)
                                }
                                wallpaper::Target::Conversation => {
                                    store.set_wallpaper(choice.clone(), cx)
                                }
                            });
                            this.wallpaper_picker = None;
                            let handle = match target {
                                wallpaper::Target::Interface => {
                                    &this.settings_focus.interface_choose
                                }
                                wallpaper::Target::Conversation => {
                                    &this.settings_focus.conversation_choose
                                }
                            };
                            window.focus(handle);
                            cx.notify();
                        }),
                    ))
            }))
    }

    fn welcome_pane(&self, cx: &Context<Self>) -> Div {
        div()
            .size_full()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p(px(24.))
            .bg(if self.store.read(cx).interface_wallpaper.is_some() {
                wallpaper::wash(if theme::is_night() { 0.88 } else { 0.80 }).into()
            } else { theme::panel_gradient() })
            .child(img("assets/icon.png").size(px(80.)))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(theme::bold())
                    .child("Good to see you."),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(theme::text_secondary())
                    .child("Choose a contact to start talking."),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .child("Ctrl+F to find someone · Ctrl+N to add a contact"),
            )
    }

    fn recent_row(
        &self,
        peer: String,
        preview: String,
        at: String,
        from_me: bool,
        unread: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = elide(&self.store.read(cx).contact_name(&peer), 26);
        let for_click = peer.clone();
        div()
            .id(gpui::SharedString::from(format!("recent-{peer}")))
            .focusable()
            .tab_index(0)
            .min_h(px(52.))
            .px(px(12.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap_3()
            .border_1()
            .border_color(gpui::rgba(0))
            .cursor_pointer()
            .hover(|style| style.bg(theme::row_hover()))
            .focus(|style| style.border_color(theme::link_color()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_chat(for_click.clone(), cx);
                if let Some(view) = this.embedded.get(&for_click) {
                    view.update(cx, |view, cx| view.focus_input(window, cx));
                }
                cx.notify();
            }))
            .child(
                img(avatar_for(&peer))
                    .w(px(28.))
                    .h(px(28.))
                    .rounded_full()
                    .object_fit(gpui::ObjectFit::Cover),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(13.))
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme::text_secondary())
                                    .child(at),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .truncate()
                            .child(if from_me {
                                format!("You: {preview}")
                            } else {
                                preview
                            }),
                    ),
            )
            .children((unread > 0).then(|| {
                div()
                    .min_w(px(20.))
                    .h(px(20.))
                    .px(px(5.))
                    .rounded_full()
                    .bg(theme::selection_bg())
                    .text_size(px(11.))
                    .text_color(theme::text_primary())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(unread.to_string())
            }))
            .into_any_element()
    }

    fn requests_section(&self, cx: &mut Context<Self>) -> Option<Div> {
        let store = self.store.read(cx);
        if store.contact_requests.is_empty() {
            return None;
        }
        let requests = store.contact_requests.clone();
        let refreshing = store.refreshing;
        let mut section = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(12.))
            .bg(theme::card_bg())
            .border_b_1()
            .border_color(theme::soft_border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(theme::bold())
                            .child("Contact requests"),
                    )
                    .child(
                        div()
                            .min_w(px(20.))
                            .h(px(20.))
                            .px(px(5.))
                            .rounded_full()
                            .bg(theme::selection_bg())
                            .text_size(px(11.))
                            .text_color(theme::text_primary())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(requests.len().to_string()),
                    )
                    .child(div().flex_1())
                    .child(
                        widgets::text_button(if refreshing { "Syncing…" } else { "Refresh" })
                            .opacity(if refreshing { 0.5 } else { 1.0 })
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(RefreshContacts), cx)
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .child("These people added you. Accept to add them back."),
            );

        for request in requests {
            let email = request.email.clone();
            let label = friendly_name(&request.display_name, &request.email);
            let show_email = !label.eq_ignore_ascii_case(&email);
            let pending = store.adding_contact.as_deref() == Some(email.as_str());
            let error = store.request_add_error(&email).map(str::to_string);
            let accept_email = email.clone();
            let ignore_email = email.clone();
            section = section.child(
                div()
                    .p(px(10.))
                    .rounded_md()
                    .bg(theme::field_bg())
                    .border_1()
                    .border_color(theme::soft_border())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                img(avatar_for(&email))
                                    .w(px(28.))
                                    .h(px(28.))
                                    .rounded_full()
                                    .object_fit(gpui::ObjectFit::Cover),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(theme::bold())
                                            .truncate()
                                            .child(label),
                                    )
                                    .children(show_email.then(|| {
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_secondary())
                                            .truncate()
                                            .child(email.clone())
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .child("wants to add you"),
                    )
                    .children(error.map(|message| {
                        div()
                            .p(px(6.))
                            .rounded_sm()
                            .bg(theme::warning_bg())
                            .text_size(px(11.))
                            .text_color(theme::danger())
                            .child(message)
                    }))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                widgets::button(if pending { "Accepting…" } else { "Accept" })
                                    .h(px(28.))
                                    .opacity(if pending { 0.5 } else { 1.0 })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.store.read(cx).adding_contact.is_some() {
                                            return;
                                        }
                                        this.store.update(cx, |store, cx| {
                                            store.add_contact(&accept_email, cx);
                                            cx.notify();
                                        });
                                        // The card disappears once the request is
                                        // accepted, so restore focus to a stable
                                        // control instead of leaving it on a
                                        // removed element.
                                        window.focus(&this.contact_search.focus_handle(cx));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::button("Not now")
                                    .h(px(28.))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.store.read(cx).adding_contact.is_some() {
                                            return;
                                        }
                                        this.store.update(cx, |store, cx| {
                                            store.dismiss_request(&ignore_email);
                                            cx.notify();
                                        });
                                        window.focus(&this.contact_search.focus_handle(cx));
                                    })),
                            ),
                    ),
            );
        }
        Some(section)
    }

    fn add_form(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.add_form_open {
            return None;
        }
        let store = self.store.read(cx);
        let pending = store.adding_contact.clone();
        let result = store.contact_add_result.clone();
        let success = result.as_ref().and_then(|result| result.as_ref().ok()).cloned();
        let success_pending = success
            .as_deref()
            .and_then(|email| store.contact(email))
            .is_some_and(|contact| contact.pending_outgoing);
        let body = match (&success, &result) {
            (Some(email), _) => {
                let label = friendly_name("", email);
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(img("assets/img/check.png").size(px(18.)))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(theme::bold())
                                    .child(if success_pending {
                                        "Request sent"
                                    } else {
                                        "Contact added"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .child(if success_pending {
                                format!(
                                    "{label} needs to add you back before you can chat. The pending badge will clear when they do."
                                )
                            } else {
                                format!("{label} is now in your contacts.")
                            }),
                    )
                    .into_any_element()
            }
            (None, Some(Err(message))) => div()
                .p(px(8.))
                .rounded_sm()
                .bg(theme::warning_bg())
                .border_1()
                .border_color(theme::warning_border())
                .text_size(px(12.))
                .text_color(theme::danger())
                .child(message.clone())
                .into_any_element(),
            _ => div()
                .text_size(px(12.))
                .text_color(theme::text_secondary())
                .child("Enter their Escargot username or email. They will receive a contact request.")
                .into_any_element(),
        };

        let buttons = if success.is_some() {
            div()
                .flex()
                .justify_end()
                .child(
                    widgets::button("Done")
                        .on_click(cx.listener(|this, _, window, cx| this.close_add_form(window, cx))),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    widgets::button("Cancel")
                        .on_click(cx.listener(|this, _, window, cx| this.close_add_form(window, cx))),
                )
                .child(
                    widgets::button(if pending.is_some() {
                        "Sending…"
                    } else {
                        "Send request"
                    })
                    .opacity(if pending.is_some() { 0.5 } else { 1.0 })
                    .on_click(cx.listener(|this, _, window, cx| this.submit_add(window, cx))),
                )
                .into_any_element()
        };

        Some(
            div()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_2()
                .bg(theme::card_bg())
                .border_b_1()
                .border_color(theme::soft_border())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .font_weight(theme::bold())
                                .child("Add a contact"),
                        )
                        .child(
                            widgets::text_button("Close")
                                .on_click(cx.listener(|this, _, window, cx| this.close_add_form(window, cx))),
                        ),
                )
                .child(body)
                .children((success.is_none()).then(|| {
                    div()
                        .h(px(36.))
                        .px(px(6.))
                        .rounded_sm()
                        .bg(theme::field_bg())
                        .border_1()
                        .border_color(theme::field_border())
                        .on_key_down(cx.listener(
                            |this, event: &gpui::KeyDownEvent, window, cx| {
                                if event.keystroke.key == "enter" {
                                    this.submit_add(window, cx);
                                    cx.stop_propagation();
                                } else if event.keystroke.key == "escape" {
                                    this.close_add_form(window, cx);
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .child(self.add_input.clone())
                }))
                .children(pending.map(|email| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::text_secondary())
                        .child(format!("Sending the request for {email}…"))
                }))
                .child(buttons),
        )
    }

    fn contacts_pane(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let (all_contacts, groups, unread_counts, recent, single, refreshing) = {
            let store = self.store.read(cx);
            (
                store.contacts.clone(),
                store.groups.clone(),
                store.unread.clone(),
                store.recent.clone(),
                store.single_window,
                store.refreshing,
            )
        };
        let query = self.contact_search.read(cx).content().trim().to_lowercase();
        let contacts: Vec<Contact> = all_contacts
            .iter()
            .filter(|contact| {
                query.is_empty()
                    || contact.label().to_lowercase().contains(&query)
                    || contact.email.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let searching = !query.is_empty();
        // Enter in the search field opens the first match, so the whole
        // contact flow stays usable from the keyboard without a pointer.
        let first_match = contacts.first().map(|contact| contact.email.clone());
        let contact_emails: HashSet<String> = all_contacts
            .iter()
            .map(|contact| contact.email.to_ascii_lowercase())
            .collect();
        let recents: Vec<_> = if searching {
            Vec::new()
        } else {
            recent
                .into_iter()
                .filter(|recent| !contact_emails.contains(&recent.peer))
                .take(12)
                .collect()
        };
        let empty = contacts.is_empty();
        let has_recents = !recents.is_empty();
        let online = contacts
            .iter()
            .filter(|contact| contact.status != Status::Offline)
            .count();

        let mut group_names: Vec<String> =
            groups.iter().map(|group| group.name.clone()).collect();
        for contact in &contacts {
            if !group_names.iter().any(|name| name == &contact.group) {
                group_names.push(contact.group.clone());
            }
        }
        group_names.sort_by_key(|name| {
            (name == "Other Contacts", name.to_lowercase())
        });

        let mut pane = div()
            .h_full()
            .flex()
            .flex_col()
            .bg(if self.store.read(cx).interface_wallpaper.is_some() {
                wallpaper::wash(0.95).into()
            } else {
                theme::list_bg()
            })
            .border_r_1()
            .border_color(theme::soft_border())
            .overflow_hidden();
        if compact {
            pane = pane.w_full().border_r_0();
        } else if single {
            pane = pane
                .w(px(300.))
                .max_w(gpui::relative(0.4))
                .flex_shrink_0();
        } else {
            pane = pane.w_full();
        }

        pane.child(
            div()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_2()
                .flex_shrink_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_weight(theme::bold())
                                .child("Contacts"),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::text_secondary())
                                .child(format!("{online}/{}", contacts.len())),
                        )
                        .child(div().flex_1())
                        .child(
                            widgets::text_button(if refreshing { "Syncing…" } else { "Sync" })
                                .opacity(if refreshing { 0.5 } else { 1.0 })
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(RefreshContacts), cx)
                                }),
                        )
                        .child(
                            widgets::button("Add contact").on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(AddContact), cx)
                            }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .h(px(34.))
                                .bg(theme::field_bg())
                                .border_1()
                                .border_color(theme::field_border())
                                .rounded_sm()
                                .px(px(6.))
                                .on_key_down(cx.listener(
                                    move |this, event: &gpui::KeyDownEvent, window, cx| {
                                        if event.keystroke.key == "enter" {
                                            if let Some(email) = first_match.clone() {
                                                this.open_contact(email, window, cx);
                                            }
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .child(self.contact_search.clone()),
                        )
                        .children(searching.then(|| {
                            widgets::text_button("×")
                                .w(px(30.))
                                .h(px(34.))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.contact_search
                                        .update(cx, |input, cx| input.clear(cx));
                                    window.focus(&this.contact_search.focus_handle(cx));
                                }))
                        })),
                ),
        )
        .child(
            div()
                .id("contacts-scroll")
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .overflow_y_scroll()
                .overflow_x_hidden()
                .children(self.add_form(cx))
                .children(self.requests_section(cx))
                .children(group_names.into_iter().filter_map(|group_name| {
                    let mut members: Vec<_> = contacts
                        .iter()
                        .filter(|contact| contact.group == group_name)
                        .cloned()
                        .collect();
                    if members.is_empty() {
                        return None;
                    }
                    members.sort_by_key(|contact| {
                        (
                            contact.status == Status::Offline,
                            contact.label().to_lowercase(),
                        )
                    });
                    let online = members
                        .iter()
                        .filter(|contact| contact.status != Status::Offline)
                        .count();
                    let open = searching
                        || groups
                            .iter()
                            .find(|group| group.name == group_name)
                            .is_none_or(|group| group.open);
                    let for_click = group_name.clone();
                    Some(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .id(gpui::SharedString::from(group_name.clone()))
                                    .focusable()
                                    .tab_index(0)
                                    .h(px(32.))
                                    .flex_shrink_0()
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::row_hover()))
                                    .focus(|style| style.bg(theme::selection_bg()))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_group(for_click.clone(), cx)
                                    }))
                                    .child(
                                        img(if open {
                                            "assets/img/group_open.png"
                                        } else {
                                            "assets/img/group_closed.png"
                                        })
                                        .size(px(11.)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(12.))
                                            .font_weight(theme::bold())
                                            .child(group_name),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_secondary())
                                            .child(format!("{online}/{}", members.len())),
                                    ),
                            )
                            .children(if open {
                                members
                                    .into_iter()
                                    .map(|contact| {
                                        let unread = unread_counts
                                            .get(&contact.email)
                                            .copied()
                                            .unwrap_or(0);
                                        let selected =
                                            self.active.as_deref() == Some(&contact.email);
                                        self.contact_row(contact, unread, selected, cx)
                                    })
                                    .collect::<Vec<_>>()
                            } else {
                                Vec::new()
                            }),
                    )
                }))
                .children(has_recents.then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .border_t_1()
                        .border_color(theme::soft_border())
                        .child(
                            div()
                                .px(px(12.))
                                .pt(px(10.))
                                .pb(px(4.))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(theme::bold())
                                        .child("Recent conversations"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(theme::text_secondary())
                                        .child("Chats stay here after a contact is removed."),
                                ),
                        )
                        .children(recents.into_iter().map(|recent| {
                            let unread = unread_counts
                                .get(&recent.peer)
                                .copied()
                                .unwrap_or(0);
                            self.recent_row(
                                recent.peer,
                                recent.preview,
                                recent.at,
                                recent.from_me,
                                unread,
                                cx,
                            )
                        }))
                }))
                .children((empty && !has_recents).then(|| {
                    if searching {
                        div()
                            .p(px(16.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(theme::bold())
                                    .child("No matching contacts"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_secondary())
                                    .child("Try a name or email address, or clear your search."),
                            )
                    } else {
                        div()
                            .p(px(20.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_2()
                            .child(
                                img("assets/img/avatar_default.png")
                                    .w(px(52.))
                                    .h(px(52.))
                                    .object_fit(gpui::ObjectFit::Contain),
                            )
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(theme::bold())
                                    .child("No contacts yet"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_secondary())
                                    .child("Add someone by username or email to start a conversation."),
                            )
                            .child(
                                widgets::button("Add a contact").on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(AddContact), cx)
                                }),
                            )
                    }
                })),
        )
    }

    fn footer(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_shrink_0()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .p(px(8.))
            .bg(theme::toolbar_gradient())
            .border_t_1()
            .border_color(theme::soft_border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h(px(32.))
                            .bg(theme::field_bg())
                            .border_1()
                            .border_color(theme::field_border())
                            .rounded_sm()
                            .px(px(6.))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" {
                                    this.open_web_search(cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .child(self.search.clone()),
                    )
                    .child(
                        widgets::button("Search web")
                            .on_click(cx.listener(|this, _, _, cx| this.open_web_search(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_0()
                    .child(
                        widgets::text_button("Solitaire")
                            .on_click(|_, _, cx| crate::ui::games::open_games_window(cx)),
                    )
                    .child(
                        widgets::text_button("Checkers")
                            .on_click(|_, _, cx| crate::ui::checkers::open_checkers_window(cx)),
                    )
                    .child(
                        widgets::text_button("Chess")
                            .on_click(|_, _, cx| crate::ui::chess::open_chess_window(cx)),
                    )
                    .child(
                        div()
                            .w(px(12.))
                            .h(px(14.))
                            .flex()
                            .justify_center()
                            .child(div().w(px(1.)).h_full().bg(theme::soft_border())),
                    )
                    .child(
                        widgets::text_button("Escargot website")
                            .on_click(|_, _, cx| browser::open("https://escargot.chat/", cx)),
                    ),
            )
    }
}

impl Render for ContactsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::scale::apply(window);
        self.scroll_settings_for_focus(window, cx);
        if let Some(email) = self.pending_demo_chat.take() {
            // Opening a window during draw corrupts GPUI's element stack on the
            // Wayland backend. Defer the demo-only open until the frame ends.
            cx.defer_in(window, move |this, _window, cx| {
                this.open_chat(email, cx);
            });
        }
        let single = self.store.read(cx).single_window;
        let compact = is_compact_layout(
            single,
            f32::from(window.bounds().size.width),
            crate::ui::scale::percent(),
        );
        let content = if self.settings_open {
            self.settings_panel(cx).into_any_element()
        } else {
            let detail = self
                .active
                .as_ref()
                .and_then(|email| self.embedded.get(email))
                .cloned();
            if compact {
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .flex()
                    .overflow_hidden()
                    .child(match detail {
                        Some(view) => div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .child(view)
                            .into_any_element(),
                        None => self.contacts_pane(true, cx).into_any_element(),
                    })
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .flex()
                    .overflow_hidden()
                    .child(self.contacts_pane(false, cx))
                    .children(single.then(|| {
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .overflow_hidden()
                            .child(match detail {
                                Some(view) => view.into_any_element(),
                                None => self.welcome_pane(cx).into_any_element(),
                            })
                    }))
                    .into_any_element()
            }
        };
        let root = self.shell(
            div()
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(content)
                .children((!compact).then(|| self.footer(cx)))
                .into_any_element(),
            cx,
        );
        if self.focus_add_input {
            // The input is mounted by this frame; focus after the tree is built.
            self.focus_add_input = false;
            window.focus(&self.add_input.focus_handle(cx));
        }
        root
    }
}
