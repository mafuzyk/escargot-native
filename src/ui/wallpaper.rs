//! Shared, keyboard-accessible choices from the bundled wallpaper collections.
use crate::ui::{scale::px, theme};
use gpui::{
    App, Div, Hsla, Img, ObjectFit, SharedString, StyledImage, Window, div, img, prelude::*,
};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Interface,
    Conversation,
}

impl Target {
    pub fn title(self) -> &'static str {
        match self {
            Self::Interface => "Interface background",
            Self::Conversation => "Conversation background",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Interface => "interface-wallpaper",
            Self::Conversation => "conversation-wallpaper",
        }
    }
}

pub fn thumbnail(name: Option<&str>) -> String {
    match name {
        None | Some("classic") => "assets/img/chat_bg.png".into(),
        Some(name) => format!("assets/wallpapers/thumbs/{name}.jpg"),
    }
}

pub fn wash(opacity: f32) -> Hsla {
    let mut color = theme::field_bg();
    color.a = opacity;
    color
}

pub fn layer(name: Option<&str>) -> Option<Img> {
    name.filter(|name| *name != "classic" && theme::wallpaper_allowed(name))
        .map(|name| {
            img(format!("assets/wallpapers/{name}.jpg"))
                .object_fit(ObjectFit::Cover)
                .absolute()
                .top(px(0.))
                .left(px(0.))
                .size_full()
        })
}

pub fn choices(
    id: &'static str,
    current: Option<String>,
    on_select: impl Fn(&Option<String>, &mut Window, &mut App) + 'static,
) -> Div {
    let select = Rc::new(on_select);
    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .children(theme::WALLPAPERS.iter().map(|&(name, label)| {
            let value = (name != "classic").then(|| name.to_string());
            let selected = current == value;
            let select = select.clone();
            div()
                .id(SharedString::from(format!("{id}-{name}")))
                .focusable()
                .tab_index(0)
                .w(px(142.))
                .p(px(4.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .rounded_sm()
                .bg(if selected {
                    theme::selection_bg()
                } else {
                    theme::field_bg()
                })
                .border_1()
                .border_color(if selected {
                    theme::accent()
                } else {
                    theme::soft_border()
                })
                .focus(|style| style.border_2().border_color(theme::link_color()))
                .cursor_pointer()
                .hover(|style| style.bg(theme::row_hover()))
                .child(
                    img(thumbnail(value.as_deref()))
                        .object_fit(ObjectFit::Cover)
                        .w_full()
                        .h(px(72.))
                        .rounded_sm(),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::text_primary())
                        .child(label),
                )
                .on_click(move |_, window, cx| select(&value, window, cx))
        }))
}
