use gpui::{
    App, Div, Img, MouseButton, MouseUpEvent, SharedString, Window, div, img, prelude::*, };
use crate::ui::scale::px;

use crate::msnp::parsing::Status;
use crate::ui::theme;

pub fn title_bar(
    title: impl Into<SharedString>,
    on_minimize: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_maximize: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_close: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let title = title.into();
    div()
        .relative()
        .h(px(26.))
        .w_full()
        .flex()
        .items_center()
        .bg(theme::title_gradient())
        .border_b_1()
        .border_color(theme::border_color())
        .pl(px(5.))
        .pr(px(3.))
        .gap_1()
        // Aero highlight: a crisp white top edge and a soft glass band.
        .child(
            div()
                .absolute()
                .top(px(0.))
                .left(px(0.))
                .w_full()
                .h(px(1.))
                .bg(gpui::rgba(0xffffffff)),
        )
        .child(
            div()
                .absolute()
                .top(px(1.))
                .left(px(0.))
                .w_full()
                .h(px(7.))
                .bg(gpui::rgba(0xffffff38)),
        )
        .child(img("assets/img/butterfly.png").w(px(18.)).h(px(16.)))
        .child(
            div()
                .font_family(theme::FONT)
                .font_weight(theme::bold())
                .italic()
                .text_size(px(13.))
                .text_color(theme::title_text())
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
        )
        .child(caption_button("–", false, on_minimize))
        .child(caption_button("□", false, on_maximize))
        .child(caption_button("×", true, on_close))
}

fn caption_button(
    glyph: &'static str,
    close: bool,
    action: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .w(px(21.))
        .h(px(19.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(theme::border_color())
        .bg(theme::aqua_button())
        .text_size(px(10.))
        .font_weight(theme::bold())
        .text_color(theme::button_text())
        .cursor_pointer()
        .hover(move |style| {
            let hover = if close {
                theme::danger()
            } else {
                theme::selection_bg()
            };
            style.bg(hover)
        })
        .on_mouse_up(MouseButton::Left, action)
        .child(glyph)
}

pub fn status_icon(status: Status) -> Img {
    status_icon_sized(status, 16.)
}

pub fn status_icon_sized(status: Status, size: f32) -> Img {
    let path = match status {
        Status::Online => "assets/img/status_online.png",
        Status::Away | Status::Idle | Status::BeRightBack => "assets/img/status_away.png",
        Status::Busy | Status::OnThePhone | Status::OutToLunch => "assets/img/status_busy.png",
        Status::Offline => "assets/img/status_offline.png",
    };
    img(path).w(px(size)).h(px(size))
}

pub fn checkbox_image(on: bool) -> Img {
    let path = if on {
        "assets/img/checkbox_on.png"
    } else {
        "assets/img/checkbox_off.png"
    };
    img(path).w(px(13.)).h(px(13.))
}

pub fn link(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT)
        .text_size(px(10.))
        .text_color(theme::link_color())
        .underline()
        .cursor_pointer()
        .hover(|style| style.text_color(theme::accent()))
        .child(text.into())
}

pub fn toolbar_button(icon: &'static str, label: &'static str) -> Div {
    div()
        .w(px(52.))
        .h(px(38.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_0()
        .rounded_sm()
        .opacity(0.55)
        .child(
            div()
                .w(px(21.))
                .h(px(21.))
                .rounded_sm()
                .bg(theme::tile_bg())
                .border_1()
                .border_color(theme::soft_border())
                .flex()
                .items_center()
                .justify_center()
                .child(img(icon).w(px(17.)).h(px(17.))),
        )
        .child(
            div()
                .font_family(theme::FONT)
                .text_size(px(8.))
                .text_color(theme::text_primary())
                .child(label),
        )
}

pub fn small_icon_button(icon: &'static str, tooltip_size: f32) -> Div {
    div()
        .w(px(tooltip_size + 6.))
        .h(px(tooltip_size + 4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(theme::row_hover()))
        .child(
            div()
                .w(px(tooltip_size + 3.))
                .h(px(tooltip_size + 3.))
                .rounded_sm()
                .bg(theme::tile_bg())
                .flex()
                .items_center()
                .justify_center()
                .child(img(icon).w(px(tooltip_size)).h(px(tooltip_size))),
        )
}

/// Low-weight action used for links and secondary toolbar entries.
pub fn text_button(label: impl Into<SharedString>) -> gpui::Stateful<Div> {
    let label = label.into();
    div()
        .id(label.clone())
        .focusable()
        .tab_index(0)
        .h(px(24.))
        .flex_shrink_0()
        .px(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .font_family(theme::FONT)
        .text_size(px(12.))
        .text_color(theme::link_color())
        .cursor_pointer()
        .hover(|style| style.bg(theme::row_hover()).underline())
        .focus(|style| style.border_1().border_color(theme::link_color()))
        .child(label)
}

pub fn separator() -> Div {
    div().h(px(1.)).w_full().bg(theme::soft_border())
}

/// Shared mouse and keyboard control for the messenger's everyday actions.
pub fn button(label: impl Into<SharedString>) -> gpui::Stateful<Div> {
    let label = label.into();
    div()
        .id(label.clone())
        .focusable()
        .tab_index(0)
        .h(px(32.))
        .flex_shrink_0()
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(theme::border_color())
        .bg(theme::aqua_button())
        .font_family(theme::FONT)
        .text_size(px(13.))
        .text_color(theme::button_text())
        .cursor_pointer()
        .hover(|style| style.bg(theme::aqua_button_hover()))
        .focus(|style| style.border_color(theme::link_color()).border_2())
        .child(label)
}

pub fn navigate_focus(event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut App) {
    if event.keystroke.key == "tab" && !event.keystroke.modifiers.control {
        if event.keystroke.modifiers.shift {
            window.focus_prev();
        } else {
            window.focus_next();
        }
        cx.stop_propagation();
    }
}
