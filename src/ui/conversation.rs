use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Context, Div, Entity, Focusable, MouseButton, Render, ScrollHandle, Window, div,
    img, prelude::*,
};
use crate::ui::scale::px;

use crate::msnp::parsing::Status;
use crate::state::{ChatMessage, Delivery, Store};
use crate::ui::SendMessage;
use crate::ui::assets::avatar_for;
use crate::ui::input::TextInput;
use crate::ui::{theme, widgets, wallpaper};

/// Original MSN 7.5 emoticons, sliced from the client's 79-cell sprite sheet.
/// Only the shortcuts we can map confidently are exposed; the rest of the
/// sheet is still shipped in `assets/emoticons` for a later pass.
const PICKER: &[(usize, &str)] = &[
    (0, ":)"),
    (1, ":D"),
    (2, ";)"),
    (3, ":O"),
    (4, ":P"),
    (5, "(H)"),
    (6, ":@"),
    (7, ":$"),
    (8, ":("),
    (9, ":'("),
    (11, ":S"),
    (12, "(6)"),
    (13, "(A)"),
    (14, "(L)"),
    (15, "(U)"),
    (19, "(S)"),
    (20, "(*)"),
    (23, "(E)"),
    (24, "(F)"),
    (27, "(K)"),
    (28, "(G)"),
    (32, "(C)"),
    (40, "(Y)"),
    (41, "(N)"),
];

pub struct ConversationView {
    store: Entity<Store>,
    peer: String,
    input: Entity<TextInput>,
    scroll: ScrollHandle,
    last_len: usize,
    last_input_len: usize,
    last_typing_at: Instant,
    picker_open: bool,
    wallpaper_open: bool,
    remove_confirmation: bool,
    add_back_confirmation: bool,
    embedded: bool,
    focused_once: bool,
}

impl ConversationView {
    pub fn new(store: Entity<Store>, peer: String, cx: &mut Context<Self>) -> Self {
        Self::build(store, peer, false, cx)
    }

    /// Conversation rendered as a tab inside the contacts window.
    pub fn new_embedded(store: Entity<Store>, peer: String, cx: &mut Context<Self>) -> Self {
        Self::build(store, peer, true, cx)
    }

    fn build(store: Entity<Store>, peer: String, embedded: bool, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("Write a message…", false, cx));
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            peer,
            input,
            scroll: ScrollHandle::new(),
            last_len: 0,
            last_input_len: 0,
            last_typing_at: Instant::now(),
            picker_open: false,
            wallpaper_open: std::env::var_os("ESCARGOT_WALLPAPER_PICKER").is_some(),
            remove_confirmation: false,
            add_back_confirmation: false,
            embedded,
            focused_once: false,
        }
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.input.focus_handle(cx));
    }

    fn send_action(&mut self, _: &SendMessage, window: &mut Window, cx: &mut Context<Self>) {
        self.send_from_composer(window, cx);
    }

    fn on_send_click(&mut self, _: &gpui::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.send_from_composer(window, cx);
    }

    fn send_from_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let reason = self.store.read(cx).send_block_reason(&self.peer);
        if let Some(reason) = reason {
            self.store.update(cx, |store, cx| {
                store.set_chat_error(&self.peer, reason);
                cx.notify();
            });
            window.focus(&self.input.focus_handle(cx));
            cx.notify();
            return;
        }
        let text = self.input.read(cx).content();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.store.update(cx, |store, cx| {
            store.send_message(&self.peer, text);
            cx.notify();
        });
        self.store.update(cx, |store, cx| {
            store.typing(&self.peer, false);
            cx.notify();
        });
        self.input.update(cx, |input, cx| input.clear(cx));
        self.last_input_len = 0;
        self.scroll.scroll_to_bottom();
        window.focus(&self.input.focus_handle(cx));
        cx.notify();
    }

    fn insert_emoticon(&mut self, code: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let mut text = self.input.read(cx).content();
        text.push_str(code);
        self.input.update(cx, |input, cx| input.set_text(text, cx));
        self.picker_open = false;
        window.focus(&self.input.focus_handle(cx));
        cx.notify();
    }

    fn header(&mut self, name: &str, cx: &mut Context<Self>) -> Div {
        let store = self.store.read(cx);
        let contact = store.contact(&self.peer).cloned();
        let status = contact.as_ref().map(|contact| contact.status).unwrap_or(Status::Offline);
        let pending = contact.as_ref().is_some_and(|contact| contact.pending_outgoing);
        let absent = contact.is_none();
        let status_text = if pending {
            "Request sent"
        } else if absent {
            "Not in contacts"
        } else if status == Status::Offline {
            "Offline"
        } else {
            status.label()
        };
        let online = !absent && !pending && status != Status::Offline;
        let addable = absent;
        div()
            .flex_shrink_0()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .min_h(px(68.))
                    .px(px(16.))
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap_3()
                    .bg(theme::panel_gradient())
                    .border_b_1()
                    .border_color(theme::soft_border())
                    .child(
                        div()
                            .relative()
                            .size(px(44.))
                            .flex_shrink_0()
                            .rounded_md()
                            .border_1()
                            .border_color(theme::soft_border())
                            .bg(theme::field_bg())
                            .p(px(1.))
                            .child(
                                img(avatar_for(&self.peer))
                                    .size_full()
                                    .rounded_md()
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
                                    .child(widgets::status_icon_sized(
                                        if online { status } else { Status::Offline },
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
                                    .text_size(px(17.))
                                    .font_weight(theme::bold())
                                    .truncate()
                                    .child(name.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_secondary())
                                    .truncate()
                                    .child(self.peer.clone()),
                            ),
                    )
                    .children((!absent).then(|| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(widgets::status_icon_sized(
                                if online { status } else { Status::Offline },
                                14.,
                            ))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(if pending {
                                        theme::link_color()
                                    } else {
                                        theme::text_secondary()
                                    })
                                    .child(status_text),
                            )
                    }))
                    .children(addable.then(|| {
                        widgets::text_button("Add contact").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.add_back_confirmation = true;
                                cx.notify();
                            },
                        ))
                    })),
            )
            .children(pending.then(|| {
                div()
                    .px(px(16.))
                    .py(px(8.))
                    .bg(theme::notice_bg())
                    .border_b_1()
                    .border_color(theme::notice_border())
                    .text_size(px(12.))
                    .text_color(theme::notice_text())
                    .child(format!(
                        "Waiting for {name} to add you back. Your message history is still here."
                    ))
            }))
            .children(absent.then(|| {
                div()
                    .px(px(16.))
                    .py(px(8.))
                    .bg(theme::notice_bg())
                    .border_b_1()
                    .border_color(theme::notice_border())
                    .text_size(px(12.))
                    .text_color(theme::notice_text())
                    .child(
                        "This person is not in your contacts. The conversation stays under Recent conversations.",
                    )
            }))
            .children((!pending && !absent && status == Status::Offline).then(|| {
                div()
                    .px(px(16.))
                    .py(px(8.))
                    .bg(theme::notice_bg())
                    .border_b_1()
                    .border_color(theme::notice_border())
                    .text_size(px(12.))
                    .text_color(theme::notice_text())
                    .child("This contact is offline. Wait until they sign in to send a message.")
            }))
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let store = self.store.read(cx);
        let contact = store.contact(&self.peer);
        let pending = contact.is_some_and(|contact| contact.pending_outgoing);
        let absent = contact.is_none();
        let removing = store.removing_contacts.contains(&self.peer);
        div()
            .flex_shrink_0()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px(px(12.))
            .py(px(7.))
            .bg(theme::toolbar_gradient())
            .border_b_1()
            .border_color(theme::soft_border())
            .child(
                widgets::button("Background").on_click(cx.listener(|this, _, _, cx| {
                    this.wallpaper_open = !this.wallpaper_open;
                    this.picker_open = false;
                    cx.notify();
                })),
            )
            .children(absent.then(|| {
                widgets::button("Add contact").on_click(cx.listener(|this, _, _, cx| {
                    this.add_back_confirmation = true;
                    cx.notify();
                }))
            }))
            .children((!absent).then(|| {
                widgets::button(if pending { "Cancel request" } else { "Remove contact" })
                    .opacity(if removing { 0.5 } else { 1.0 })
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.store.read(cx).removing_contacts.contains(&this.peer) {
                            return;
                        }
                        this.remove_confirmation = true;
                        cx.notify();
                    }))
            }))
            .child(div().flex_1())
    }

    fn removal_confirmation(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.remove_confirmation {
            return None;
        }
        let store = self.store.read(cx);
        let name = store.contact_name(&self.peer);
        let pending = store
            .contact(&self.peer)
            .is_some_and(|contact| contact.pending_outgoing);
        Some(
            div()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_2()
                .flex_shrink_0()
                .bg(theme::warning_bg())
                .border_b_1()
                .border_color(theme::warning_border())
                .text_color(theme::text_primary())
                .child(
                    div()
                        .font_weight(theme::bold())
                        .child(if pending {
                            format!("Cancel the request to {name}?")
                        } else {
                            format!("Remove {name} from your contacts?")
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .child(if pending {
                            "They will not be added. The conversation stays under Recent conversations."
                        } else {
                            "The conversation stays under Recent conversations and your history is kept. This does not block them."
                        }),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            widgets::button("Keep").on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.remove_confirmation = false;
                                    this.focus_input(window, cx);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            widgets::button(if pending { "Cancel request" } else { "Remove" })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.remove_confirmation = false;
                                    this.store
                                        .update(cx, |store, cx| store.remove_contact(&this.peer, cx));
                                    this.focus_input(window, cx);
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    fn add_back_confirmation(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.add_back_confirmation || self.store.read(cx).contact(&self.peer).is_some() {
            return None;
        }
        let name = self.store.read(cx).contact_name(&self.peer);
        let pending = self.store.read(cx).adding_contact.is_some();
        Some(
            div()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_2()
                .flex_shrink_0()
                .bg(theme::notice_bg())
                .border_b_1()
                .border_color(theme::notice_border())
                .child(
                    div()
                        .font_weight(theme::bold())
                        .child(format!("Add {name} back to your contacts?")),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .child("They will receive a contact request. The existing history is kept."),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            widgets::button("Cancel").on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.add_back_confirmation = false;
                                    this.focus_input(window, cx);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            widgets::button(if pending { "Sending…" } else { "Send request" })
                                .opacity(if pending { 0.5 } else { 1.0 })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.store.read(cx).adding_contact.is_some() {
                                        return;
                                    }
                                    this.store.update(cx, |store, cx| {
                                        store.add_contact(&this.peer, cx);
                                        cx.notify();
                                    });
                                    window.focus(&this.input.focus_handle(cx));
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    fn chat_error_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let store = self.store.read(cx);
        let message = store.chat_errors.get(&self.peer)?.clone();
        let has_failed = store.has_failed_messages(&self.peer);
        Some(
            div()
                .flex_shrink_0()
                .px(px(12.))
                .py(px(7.))
                .flex()
                .items_center()
                .gap_2()
                .bg(theme::warning_bg())
                .border_t_1()
                .border_color(theme::warning_border())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .text_color(theme::danger())
                        .child(message),
                )
                .children(has_failed.then(|| {
                    widgets::text_button("Retry failed")
                        .text_color(theme::danger())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.store.update(cx, |store, cx| {
                                store.retry_failed_messages(&this.peer);
                                cx.notify();
                            });
                            this.focus_input(window, cx);
                        }))
                }))
                .child(
                    widgets::text_button("Dismiss")
                        .text_color(theme::text_secondary())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store.update(cx, |store, cx| {
                                store.clear_chat_error(&this.peer);
                                cx.notify();
                            });
                        })),
                ),
        )
    }

    fn composer(&mut self, cx: &mut Context<Self>) -> Div {
        let store = self.store.read(cx);
        let typing = store.typing.get(&self.peer).copied().unwrap_or(false);
        let reason = store.send_block_reason(&self.peer);
        let hint = if let Some(reason) = &reason {
            reason.clone()
        } else if typing {
            format!("{} is typing…", store.contact_name(&self.peer))
        } else {
            "Enter to send".into()
        };
        let empty = self.input.read(cx).content().trim().is_empty();
        let send_ready = !empty && reason.is_none();
        div()
            .flex_shrink_0()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(12.))
            .border_t_1()
            .border_color(theme::soft_border())
            .bg(theme::panel_gradient())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        widgets::button("Emoticons").on_click(cx.listener(|this, _, _, cx| {
                            this.picker_open = !this.picker_open;
                            this.wallpaper_open = false;
                            cx.notify();
                        })),
                    )
                    .child(
                        widgets::button("Nudge").on_click(cx.listener(|this, _, window, cx| {
                            if let Some(reason) = this.store.read(cx).send_block_reason(&this.peer) {
                                this.store.update(cx, |store, cx| {
                                    store.set_chat_error(&this.peer, reason);
                                    cx.notify();
                                });
                                this.focus_input(window, cx);
                                return;
                            }
                            this.store.update(cx, |store, cx| {
                                store.send_nudge(&this.peer);
                                cx.notify();
                            });
                            play_sound("assets/sounds/nudge.wav");
                            this.focus_input(window, cx);
                        })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .max_w(gpui::relative(0.6))
                            .truncate()
                            .text_size(px(11.))
                            .text_color(if reason.is_some() {
                                theme::notice_text()
                            } else {
                                theme::text_secondary()
                            })
                            .child(hint),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h(px(42.))
                            .px(px(8.))
                            .rounded_sm()
                            .bg(theme::field_bg())
                            .border_1()
                            .border_color(theme::field_border())
                            .child(self.input.clone()),
                    )
                    .child(
                        widgets::button("Send")
                            .h(px(42.))
                            .opacity(if send_ready { 1.0 } else { 0.5 })
                            .on_click(cx.listener(Self::on_send_click)),
                    ),
            )
    }

    fn wallpaper_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.store.read(cx).wallpaper.clone();
        div()
            .id("wallpaper-picker")
            .absolute()
            .top(px(112.))
            .left(px(10.))
            .w(px(470.))
            .max_w(gpui::relative(0.96))
            .max_h(gpui::relative(0.62))
            .overflow_y_scroll()
            .p(px(10.))
            .rounded_md()
            .bg(theme::field_bg())
            .border_1()
            .border_color(theme::border_color())
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(theme::bold())
                            .child("Conversation background"),
                    )
                    .child(
                        widgets::text_button("Close").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.wallpaper_open = false;
                                this.focus_input(window, cx);
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .child("Applies to every conversation. The contact list keeps its own background in Settings."),
            )
            .child(wallpaper::choices(
                "chat-background",
                current,
                cx.listener(|this, choice: &Option<String>, window, cx| {
                    this.store.update(cx, |store, cx| store.set_wallpaper(choice.clone(), cx));
                    this.wallpaper_open = false;
                    this.focus_input(window, cx);
                    cx.notify();
                }),
            ))
    }

    fn message_row(
        &self,
        message: &ChatMessage,
        contact_name: &str,
        self_name: &str,
        name_color: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if message.system {
            return div()
                .w_full()
                .flex()
                .justify_center()
                .py(px(2.))
                .child(
                    div()
                        .px(px(10.))
                        .py(px(3.))
                        .rounded_full()
                        .bg(theme::system_pill_bg())
                        .font_family(theme::FONT)
                        .text_size(px(11.))
                        .italic()
                        .text_color(theme::text_secondary())
                        .child(message.text.clone()),
                )
                .into_any_element();
        }

        let name = if message.from_me {
            self_name
        } else {
            contact_name
        };
        let color = if message.from_me {
            theme::link_color()
        } else {
            name_color
        };
        let failed = matches!(message.delivery, Delivery::Failed(_));
        let peer = self.peer.clone();
        let message_id = message.id;
        let mut row = div()
            .flex()
            .flex_col()
            .gap_1()
            .py(px(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(12.))
                            .font_weight(theme::bold())
                            .text_color(color)
                            .child(name.to_string()),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(10.))
                            .text_color(theme::text_secondary())
                            .child(message.at.clone()),
                    )
                    .children(message.from_me.then(|| {
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(10.))
                            .text_color(if failed {
                                theme::danger()
                            } else {
                                theme::text_secondary()
                            })
                            .child(match &message.delivery {
                                Delivery::Sending => "Sending…",
                                Delivery::Sent => "Sent",
                                Delivery::Failed(_) => "Not sent",
                            })
                    })),
            )
            .child(rich_text(&message.text, 14.));
        if failed {
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::danger())
                            .child("Escargot did not confirm this message."),
                    )
                    .child(
                        widgets::text_button("Retry")
                            .h(px(20.))
                            .text_color(theme::danger())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.store.update(cx, |store, cx| {
                                    store.retry_message(&peer, message_id);
                                    cx.notify();
                                });
                                this.focus_input(window, cx);
                            })),
                    ),
            );
        }
        row.into_any_element()
    }

    fn picker(&self, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .left(px(8.))
            .bottom(px(124.))
            .w(px(274.))
            .flex()
            .flex_wrap()
            .gap_1()
            .p(px(6.))
            .bg(theme::field_bg())
            .border_1()
            .border_color(theme::border_color())
            .shadow_lg()
            .children(PICKER.iter().map(|(index, code)| {
                let index = *index;
                let code = *code;
                div()
                    .w(px(27.))
                    .h(px(27.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::row_hover()))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.insert_emoticon(code, window, cx)
                        }),
                    )
                    .child(
                        img(format!("assets/emoticons/{:03}.png", index))
                            .w(px(19.))
                            .h(px(19.)),
                    )
                    .into_any_element()
            }))
    }
}

impl Render for ConversationView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::scale::apply(window);
        if !self.focused_once {
            self.focused_once = true;
            self.focus_input(window, cx);
        }
        let (messages, contact_name, self_name, wallpaper, night) = {
            let store = self.store.read(cx);
            (
                store.messages_for(&self.peer).to_vec(),
                store.contact_name(&self.peer),
                store.own_label(),
                store.wallpaper.clone(),
                store.night,
            )
        };

        if messages.len() != self.last_len {
            self.last_len = messages.len();
            self.scroll.scroll_to_bottom();
        }

        let input_len = self.input.read(cx).content().chars().count();
        if input_len != self.last_input_len {
            self.last_input_len = input_len;
            if input_len > 0 && self.last_typing_at.elapsed() > Duration::from_secs(2) {
                self.last_typing_at = Instant::now();
                self.store.update(cx, |store, cx| {
                    store.typing(&self.peer, true);
                    cx.notify();
                });
            }
        }

        let title = format!("{} - Conversation", contact_name);
        let name_color = theme::nickname_color(&self.peer);
        let background = match wallpaper.as_deref() {
            Some(name) if theme::wallpaper_allowed(name) => {
                Some(format!("assets/wallpapers/{name}.jpg"))
            }
            Some(_) => None,
            None if !night => Some("assets/img/chat_bg.png".to_string()),
            None => None,
        };

        div()
            .key_context("MessageComposer")
            .on_action(cx.listener(Self::send_action))
            .on_key_down(widgets::navigate_focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && (this.picker_open
                        || this.wallpaper_open
                        || this.remove_confirmation
                        || this.add_back_confirmation)
                {
                    this.picker_open = false;
                    this.wallpaper_open = false;
                    this.remove_confirmation = false;
                    this.add_back_confirmation = false;
                    this.focus_input(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::field_bg())
            .font_family(theme::FONT)
            .text_size(px(13.))
            .text_color(theme::text_primary())
            .child(if self.embedded {
                div().h(px(0.)).into_any_element()
            } else {
                widgets::title_bar(
                    title,
                    |_, window, _| window.minimize_window(),
                    |_, window, _| window.toggle_fullscreen(),
                    |_, window, _| window.remove_window(),
                )
                .into_any_element()
            })
            .child(self.header(&contact_name, cx))
            .child(self.toolbar(cx))
            .children(self.removal_confirmation(cx))
            .children(self.add_back_confirmation(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .flex()
                    .relative()
                    .bg(theme::field_bg())
                    .children(background.as_ref().map(|path| {
                        img(path.clone())
                            .absolute()
                            .top(px(0.))
                            .left(px(0.))
                            .w_full()
                            .h_full()
                            .object_fit(gpui::ObjectFit::Cover)
                            .into_any_element()
                    }))
                    .children(background.is_some().then(|| {
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(px(0.))
                            .w_full()
                            .h_full()
                            .bg(theme::chat_scrim())
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .id("conversation-messages")
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .p(px(16.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(messages.iter().map(|message| {
                                self.message_row(message, &contact_name, &self_name, name_color, cx)
                            })),
                    ),
            )
            .children(self.chat_error_banner(cx))
            .child(self.composer(cx))
            .children(if self.picker_open {
                Some(self.picker(cx))
            } else {
                None
            })
            .children(if self.wallpaper_open {
                Some(self.wallpaper_picker(cx))
            } else {
                None
            })
    }
}

fn rich_text(text: &str, size: f32) -> Div {
    let mut row = div().flex().flex_wrap().items_center().gap_0();

    for token in tokenize(text) {
        match token {
            Token::Text(value) => {
                for word in value.split_inclusive(' ') {
                    row = row.child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(size))
                            .line_height(px(size + 4.))
                            .text_color(theme::text_primary())
                            .whitespace_nowrap()
                            .child(word.to_string()),
                    );
                }
            }
            Token::Emote(index) => {
                row = row.child(
                    div()
                        .w(px(size + 7.))
                        .h(px(size + 5.))
                        .flex()
                        .items_center()
                        .child(
                            img(format!("assets/emoticons/{:03}.png", index))
                                .w(px(size + 4.))
                                .h(px(size + 4.)),
                        ),
                );
            }
        }
    }

    row
}

enum Token {
    Text(String),
    Emote(usize),
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = text;

    'outer: while !rest.is_empty() {
        for &(index, code) in PICKER {
            if rest.starts_with(code) {
                tokens.push(Token::Emote(index));
                rest = &rest[code.len()..];
                continue 'outer;
            }
        }

        let next = PICKER
            .iter()
            .filter_map(|(_, code)| rest.find(*code))
            .min()
            .unwrap_or(rest.len());
        if next == 0 {
            break;
        }
        tokens.push(Token::Text(rest[..next].to_string()));
        rest = &rest[next..];
    }

    tokens
}

fn play_sound(relative: &str) {
    let path = crate::ui::assets::asset_path(relative);
    let _ = Command::new("pw-play")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
