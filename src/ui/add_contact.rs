use gpui::{
    AnyWindowHandle, App, Bounds, ClickEvent, Context, Entity, FocusHandle, Focusable, Render, Window, WindowBounds,
    WindowDecorations, WindowOptions, div, img, prelude::*, size,
};
use crate::ui::scale::px;

use crate::state::Store;
use crate::ui::SubmitContact;
use crate::ui::input::TextInput;
use crate::ui::{theme, widgets};

pub fn open_add_contact_window(store: Entity<Store>, initial: String, parent: Option<(AnyWindowHandle, FocusHandle)>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(gpui::px(440.), gpui::px(350.)), cx);
    if let Ok(handle) = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("escargot-native-add-contact".into()),
            window_decorations: Some(WindowDecorations::Client),
            is_resizable: false,
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| AddContactView::new(store, initial, parent, cx)),
    ) {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}

pub struct AddContactView {
    store: Entity<Store>,
    account: Entity<TextInput>,
    focus: FocusHandle,
    focused_once: bool,
    parent: Option<(AnyWindowHandle, FocusHandle)>,
}

impl AddContactView {
    fn new(store: Entity<Store>, initial: String, parent: Option<(AnyWindowHandle, FocusHandle)>, cx: &mut Context<Self>) -> Self {
        let prefill = store.read(cx).adding_contact.clone().unwrap_or(initial);
        store.update(cx, |store, cx| {
            if store.adding_contact.is_none() {
                store.contact_add_result = None;
            }
            cx.notify();
        });
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let account = cx.new(|cx| {
            let mut input = TextInput::new("username or email", false, cx);
            input.set_text(prefill, cx);
            input
        });
        Self {
            store,
            account,
            focus: cx.focus_handle(),
            focused_once: false,
            parent,
        }
    }

    fn submit_action(&mut self, _: &SubmitContact, window: &mut Window, cx: &mut Context<Self>) {
        self.submit(window, cx);
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        if store.adding_contact.is_some() || matches!(store.contact_add_result, Some(Ok(_))) {
            return;
        }
        let account = self.account.read(cx).content();
        self.store
            .update(cx, |store, cx| store.add_contact(&account, cx));
        if self
            .store
            .read(cx)
            .contact_add_result
            .as_ref()
            .is_some_and(|result| result.is_err())
        {
            window.focus(&self.account.focus_handle(cx));
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        if let Some((parent, focus)) = self.parent.clone() {
            parent.update(cx, |_, window, _| {
                window.focus(&focus);
                window.activate_window();
            }).ok();
        }
    }

    fn close(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(window, cx);
    }
}

impl Render for AddContactView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::scale::apply(window);
        let result = self.store.read(cx).contact_add_result.clone();
        let pending = self.store.read(cx).adding_contact.clone();
        let success = matches!(result, Some(Ok(_)));
        if success {
            window.focus(&self.focus);
        } else if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.account.focus_handle(cx));
        }
        let content = match result {
            Some(Ok(email)) => div().flex().flex_col().gap_3()
                .child(div().flex().items_center().gap_2()
                    .child(img("assets/img/check.png").size(px(20.)))
                    .child(div().text_size(px(18.)).font_weight(theme::bold()).child("Contact added")))
                .child(div().child(email))
                .child(div().text_color(theme::text_secondary()).child("They are now in your contacts. They may appear offline until they add you back."))
                .child(div().flex().justify_end().child(widgets::button("Done").on_click(cx.listener(Self::close))))
                .into_any_element(),
            result => div().flex().flex_col().gap_3()
                .child(div().flex().items_center().gap_2()
                    .child(img("assets/img/add_contact.png").size(px(24.)))
                    .child(div().text_size(px(18.)).font_weight(theme::bold()).child("Add a contact")))
                .child(div().text_color(theme::text_secondary()).child("Add someone directly to your MSN contact list."))
                .child(div().child("Escargot username or email"))
                .child(div().key_context("AddContactInput").h(px(36.)).bg(theme::field_bg())
                    .border_1().border_color(theme::field_border()).rounded_sm().px(px(6.)).child(self.account.clone()))
                .children(match (pending.as_ref(), result) {
                    (Some(email), _) => Some(div().text_color(theme::text_secondary()).child(format!("Adding {email}…")).into_any_element()),
                    (_, Some(Err(message))) => Some(div().p(px(10.)).rounded_sm().bg(theme::warning_bg()).text_color(theme::danger()).child(message).into_any_element()),
                    _ => None,
                })
                .child(div().mt(px(4.)).flex().justify_end().gap_2()
                    .child(widgets::button(if pending.is_some() { "Close" } else { "Cancel" }).on_click(cx.listener(Self::close)))
                    .child(widgets::button(if pending.is_some() { "Adding…" } else { "Add contact" })
                        .opacity(if pending.is_some() { 0.5 } else { 1.0 })
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))))
                .into_any_element(),
        };
        div()
            .key_context("AddContactDialog")
            .track_focus(&self.focus)
            .tab_stop(false)
            .on_action(cx.listener(Self::submit_action))
            .on_key_down(widgets::navigate_focus)
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape"
                        || (success && event.keystroke.key == "enter")
                    {
                        this.dismiss(window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::panel_gradient())
            .font_family(theme::FONT)
            .text_size(px(13.))
            .text_color(theme::text_primary())
            .border_1()
            .border_color(theme::border_color())
            .child(widgets::title_bar(
                "Add a Contact",
                |_, w, _| w.minimize_window(),
                |_, w, _| w.toggle_fullscreen(),
                cx.listener(|this, _, window, cx| this.dismiss(window, cx)),
            ))
            .child(
                div()
                    .id("add-contact-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(24.))
                    .child(content),
            )
    }
}
