use gpui::Focusable;
use gpui::{
    App, Bounds, Context, Entity, Render, SharedString, Window, WindowBounds, WindowDecorations,
    WindowOptions, div, img, prelude::*, size,
};
use crate::ui::scale::px;

use crate::config;
use crate::msnp::parsing::Status;
use crate::state::Store;
use crate::ui::SignIn;
use crate::ui::input::TextInput;
use crate::ui::{browser, contacts, theme, widgets};

pub fn open_login_window(store: Entity<Store>, cx: &mut App) {
    store.update(cx, |store, cx| {
        store.signed_in = false;
        store.connecting = false;
        store.connection_error = None;
        store.login_error = None;
        cx.notify();
    });

    let bounds = Bounds::centered(None, size(gpui::px(400.), gpui::px(680.)), cx);
    let store_for_window = store.clone();
    if let Ok(handle) = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("escargot-native-login".into()),
            window_decorations: Some(WindowDecorations::Client),
            // Zooming to 200% makes the sign-in form taller than the default
            // window, so allow resizing and keep the form scrollable.
            is_resizable: true,
            window_min_size: Some(size(gpui::px(340.), gpui::px(420.))),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| LoginView::new(store_for_window.clone(), cx)),
    ) {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}

pub struct LoginView {
    store: Entity<Store>,
    email: Entity<TextInput>,
    password: Entity<TextInput>,
    remember: bool,
    remember_password: bool,
    auto_sign_in: bool,
    status: Status,
    error: Option<SharedString>,
    focused_once: bool,
}

impl LoginView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let settings = config::load();
        let store_email = store.read(cx).email.clone();
        let prefilled = if !store_email.is_empty() {
            store_email
        } else {
            settings.email.clone().unwrap_or_default()
        };
        let saved_password = if settings.remember_password && !prefilled.is_empty() {
            config::load_password(&prefilled)
        } else {
            None
        };
        let email_prefill = prefilled.clone();
        let email = cx.new(move |cx| {
            let mut input = TextInput::new("username", false, cx);
            if !email_prefill.is_empty() {
                input.set_text(email_prefill, cx);
            }
            input
        });
        let password = cx.new(move |cx| {
            let mut input = TextInput::new("", true, cx);
            if let Some(password) = saved_password {
                input.set_text(password, cx);
            }
            input
        });

        Self {
            store,
            email,
            password,
            remember: settings.remember_me || settings.email.is_some(),
            remember_password: settings.remember_password,
            auto_sign_in: settings.auto_sign_in,
            status: Status::Online,
            error: None,
            focused_once: false,
        }
    }

    fn sign_in_action(&mut self, _: &SignIn, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_in(window, cx);
    }

    fn sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.email.read(cx).content();
        let password = self.password.read(cx).content();
        let status = self.status;

        let ok = self.store.update(cx, |store, cx| {
            let ok = store.sign_in(&email, &password, cx);
            if ok {
                store.set_status(status);
            }
            cx.notify();
            ok
        });

        if ok {
            let account = self.store.read(cx).email.clone();
            let wants_password = self.remember_password || self.auto_sign_in;
            let save_email = self.remember || self.auto_sign_in;
            let mut settings = config::load();
            settings.email = if save_email {
                Some(account.clone())
            } else {
                None
            };
            settings.remember_me = save_email;
            settings.remember_password = wants_password;
            settings.auto_sign_in = self.auto_sign_in;
            let _ = config::save(&settings);
            if wants_password {
                config::store_password(&account, &password);
            } else {
                config::clear_password(&account);
            }

            contacts::open_contacts_window(self.store.clone(), cx);
            window.remove_window();
        } else {
            self.error = self.store.read(cx).login_error.clone();
            cx.notify();
        }
    }

    fn on_sign_in_click(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sign_in(window, cx);
    }

    fn check_row(
        &self,
        label: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
        toggle: fn(&mut Self),
    ) -> impl IntoElement {
        div()
            .id(label)
            .focusable()
            .tab_index(0)
            .min_h(px(32.))
            .flex()
            .items_center()
            .gap_2()
            .px(px(4.))
            .border_1()
            .border_color(gpui::rgba(0))
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(theme::row_hover()))
            .focus(|style| style.border_color(theme::link_color()))
            .on_click(cx.listener(move |this, _, _, cx| {
                toggle(this);
                cx.notify();
            }))
            .child(widgets::checkbox_image(checked))
            .child(label)
    }

    fn input_box(&self, input: Entity<TextInput>) -> gpui::Div {
        div()
            .w_full()
            .h(px(36.))
            .bg(theme::field_bg())
            .border_1()
            .border_color(theme::field_border())
            .rounded_sm()
            .px(px(6.))
            .child(input)
    }
}

impl Render for LoginView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::scale::apply(window);
        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.email.focus_handle(cx));
        }
        div()
            .key_context("LoginWindow")
            .on_action(cx.listener(Self::sign_in_action))
            .on_key_down(widgets::navigate_focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::login_gradient())
            .font_family(theme::FONT)
            .text_size(px(13.))
            .text_color(theme::text_primary())
            .border_1()
            .border_color(theme::border_color())
            .child(widgets::title_bar(
                "MSN Messenger",
                |_, w, _| w.minimize_window(),
                |_, w, _| w.toggle_fullscreen(),
                |_, w, _| w.remove_window(),
            ))
            .child(
                div()
                    .id("login-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .p(px(32.))
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(img("assets/img/avatar_default.png").size(px(64.)))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(20.))
                                                    .font_weight(theme::bold())
                                                    .child("Sign in to Escargot"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(theme::text_secondary())
                                                    .child("Your MSN conversations, back online."),
                                            ),
                                    ),
                            )
                            .child(div().mt(px(12.)).child("Escargot username or email"))
                            .child(self.input_box(self.email.clone()))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_secondary())
                                    .child("A username is enough; we add @escargot.chat."),
                            )
                            .child(div().mt(px(4.)).child("Password"))
                            .child(self.input_box(self.password.clone()))
                            .child(
                                div().flex().flex_col().gap_2().child("Sign in as").child(
                                    div().flex().flex_wrap().gap_2().children(
                                        [
                                            (Status::Online, "Online"),
                                            (Status::Away, "Away"),
                                            (Status::Busy, "Busy"),
                                            (Status::Offline, "Offline"),
                                        ]
                                        .into_iter()
                                        .map(
                                            |(status, label)| {
                                                widgets::button(label)
                                                    .h(px(28.))
                                                    .px(px(8.))
                                                    .border_color(if self.status == status {
                                                        theme::link_color()
                                                    } else {
                                                        theme::border_color()
                                                    })
                                                    .bg(if self.status == status {
                                                        theme::selection_bg()
                                                    } else {
                                                        theme::field_bg()
                                                    })
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.status = status;
                                                        cx.notify();
                                                    }))
                                            },
                                        ),
                                    ),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(self.check_row(
                                                "Remember my account",
                                                self.remember,
                                                cx,
                                                |this| this.remember = !this.remember,
                                            ))
                                            .child(
                                                widgets::button("Forget me")
                                                    .h(px(28.))
                                                    .px(px(6.))
                                                    .bg(gpui::rgba(0))
                                                    .border_color(gpui::rgba(0))
                                                    .text_color(theme::link_color())
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            let account =
                                                                this.email.read(cx).content();
                                                            this.email.update(cx, |input, cx| {
                                                                input.clear(cx)
                                                            });
                                                            this.password
                                                                .update(cx, |input, cx| {
                                                                    input.clear(cx)
                                                                });
                                                            this.error = None;
                                                            this.remember = false;
                                                            this.remember_password = false;
                                                            this.auto_sign_in = false;
                                                            config::clear_password(
                                                                &crate::msnp::normalize_account(
                                                                    &account,
                                                                ),
                                                            );
                                                            config::forget_email();
                                                            window.focus(
                                                                &this.email.focus_handle(cx),
                                                            );
                                                            cx.notify();
                                                        },
                                                    )),
                                            ),
                                    )
                                    .child(self.check_row(
                                        "Remember my password",
                                        self.remember_password,
                                        cx,
                                        |this| this.remember_password = !this.remember_password,
                                    ))
                                    .child(self.check_row(
                                        "Sign me in automatically",
                                        self.auto_sign_in,
                                        cx,
                                        |this| this.auto_sign_in = !this.auto_sign_in,
                                    )),
                            )
                            .children(self.error.clone().map(|error| {
                                div()
                                    .p(px(10.))
                                    .bg(theme::warning_bg())
                                    .rounded_sm()
                                    .text_size(px(12.))
                                    .text_color(theme::danger())
                                    .child(error)
                            }))
                            .child(
                                widgets::button("Sign in")
                                    .h(px(40.))
                                    .w_full()
                                    .font_weight(theme::bold())
                                    .on_click(cx.listener(Self::on_sign_in_click)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme::soft_border())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                widgets::button("Account help").on_click(|_, _, cx| {
                                    browser::open("https://escargot.chat/", cx)
                                }),
                            )
                            .child(widgets::button("Create account").on_click(|_, _, cx| {
                                browser::open("https://escargot.chat/register", cx)
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .child("Unofficial client for the Escargot network."),
                    ),
            )
    }
}
