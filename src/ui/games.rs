//! Offline Solitaire window. Cards are PNG assets; the table itself is drawn
//! with GPUI surfaces so it matches the Aero chrome.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, App, Bounds, Context, Div, MouseButton, MouseUpEvent, Render, Window, WindowBounds,
    WindowDecorations, WindowOptions, div, img, prelude::*, rgba, size,
};
use crate::ui::scale::px;

use crate::games::solitaire::{Game, Suit, TABLEAU};
use crate::ui::{theme, widgets};

const CARD_W: f32 = 76.;
const CARD_H: f32 = 106.;
const COL_W: f32 = 84.;
const FACE_UP_STEP: f32 = 24.;
const FACE_DOWN_STEP: f32 = 9.;

#[derive(Clone, Copy)]
enum Pick {
    Waste,
    Tableau(usize, usize),
}

pub struct GamesView {
    game: Game,
    selected: Option<Pick>,
    seed: u64,
}

pub fn open_games_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(gpui::px(720.), gpui::px(600.)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("escargot-native-games".into()),
            window_decorations: Some(WindowDecorations::Client),
            is_resizable: true,
            window_min_size: Some(size(gpui::px(660.), gpui::px(520.))),
            ..Default::default()
        },
        |_, cx| cx.new(GamesView::new),
    );
    if let Ok(handle) = handle {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}

impl GamesView {
    fn new(_cx: &mut Context<Self>) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_nanos() as u64)
            .unwrap_or(7);
        Self {
            game: Game::new(seed),
            selected: None,
            seed,
        }
    }

    fn new_game(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.seed = self.seed.wrapping_add(1);
        self.game = Game::new(self.seed);
        self.selected = None;
        cx.notify();
    }

    fn undo(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.game.undo() {
            self.selected = None;
        }
        cx.notify();
    }

    fn draw_card(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.game.draw();
        self.selected = None;
        cx.notify();
    }

    fn click_waste(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.click_count >= 2 && self.game.move_waste_to_foundation() {
            self.selected = None;
            cx.notify();
            return;
        }
        self.selected = match self.selected {
            Some(Pick::Waste) => None,
            _ if self.game.waste_top().is_some() => Some(Pick::Waste),
            _ => None,
        };
        cx.notify();
    }

    fn click_foundation(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let moved = match self.selected {
            Some(Pick::Waste) => self.game.move_waste_to_foundation(),
            Some(Pick::Tableau(column, index)) => {
                index + 1 == self.game.tableau(column).len()
                    && self.game.move_tableau_to_foundation(column)
            }
            None => false,
        };
        if moved {
            self.selected = None;
        }
        cx.notify();
    }

    fn click_tableau(
        &mut self,
        column: usize,
        index: usize,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.click_count >= 2
            && index + 1 == self.game.tableau(column).len()
            && self.game.move_tableau_to_foundation(column)
        {
            self.selected = None;
            cx.notify();
            return;
        }
        let face_up = self
            .game
            .tableau(column)
            .get(index)
            .map(|card| card.face_up)
            .unwrap_or(false);

        let moved = match self.selected {
            Some(Pick::Waste) => self.game.move_waste_to_tableau(column),
            Some(Pick::Tableau(from, start)) => self.game.move_run(from, start, column),
            None => false,
        };

        if moved {
            self.selected = None;
        } else if face_up {
            self.selected = Some(Pick::Tableau(column, index));
        }
        cx.notify();
    }

    fn click_empty_column(
        &mut self,
        column: usize,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let moved = match self.selected {
            Some(Pick::Waste) => self.game.move_waste_to_tableau(column),
            Some(Pick::Tableau(from, start)) => self.game.move_run(from, start, column),
            None => false,
        };
        if moved {
            self.selected = None;
        }
        cx.notify();
    }

    fn selected_card(&self) -> Option<(usize, usize)> {
        match self.selected {
            Some(Pick::Tableau(column, index)) => Some((column, index)),
            _ => None,
        }
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(48.))
            .flex_shrink_0()
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px(px(8.))
            .bg(theme::toolbar_gradient())
            .border_b_1()
            .border_color(theme::soft_border())
            .child(self.toolbar_button("New game", cx.listener(Self::new_game)))
            .child(self.toolbar_button("Undo", cx.listener(Self::undo)))
            .child(div().flex_1())
            .child(
                div()
                    .font_family(theme::FONT)
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .child(format!(
                        "Score {}   Moves {}",
                        self.game.score, self.game.moves
                    )),
            )
    }

    fn toolbar_button(
        &self,
        label: &'static str,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        widgets::button(label).on_click(handler)
    }

    fn slot(&self, path: Option<String>, selected: bool) -> Div {
        let mut slot = div()
            .w(px(CARD_W))
            .h(px(CARD_H))
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme::accent()
            } else {
                rgba(0xffffff55).into()
            })
            .bg(rgba(0x00000022));
        if let Some(path) = path {
            slot = slot.child(img(path).w(px(CARD_W)).h(px(CARD_H)));
        }
        slot
    }

    fn foundation(&self, suit: Suit, cx: &mut Context<Self>) -> Div {
        let top = self.game.foundation(suit).last().copied();
        self.slot(top.map(|card| card.asset()), false)
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, cx.listener(Self::click_foundation))
    }

    fn tableau_column(&self, column: usize, cx: &mut Context<Self>) -> AnyElement {
        let pile = self.game.tableau(column);
        let selected = self.selected_card().filter(|(col, _)| *col == column);

        if pile.is_empty() {
            return self
                .slot(None, false)
                .flex()
                .items_center()
                .justify_center()
                .font_family(theme::FONT)
                .text_size(px(16.))
                .text_color(rgba(0xffffff55))
                .cursor_pointer()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, event, window, cx| {
                        this.click_empty_column(column, event, window, cx)
                    }),
                )
                .into_any_element();
        }

        let mut top = 0.;
        let mut cards = Vec::new();
        for (index, pile_card) in pile.iter().enumerate() {
            let is_selected = selected.map(|(_, start)| start == index).unwrap_or(false);
            let path = if pile_card.face_up {
                pile_card.card.asset()
            } else {
                "assets/games/cards/back.png".to_string()
            };
            let card = self.slot(Some(path), is_selected);
            cards.push(
                div()
                    .absolute()
                    .top(px(top))
                    .left(px(2.))
                    .cursor_pointer()
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.click_tableau(column, index, event, window, cx)
                        }),
                    )
                    .child(card)
                    .into_any_element(),
            );
            top += if pile_card.face_up {
                FACE_UP_STEP
            } else {
                FACE_DOWN_STEP
            };
        }

        div()
            .relative()
            .w(px(COL_W))
            .h_full()
            .children(cards)
            .into_any_element()
    }
}

impl Render for GamesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::scale::apply(window);
        let waste = self.game.waste_top();
        let stock = self.game.stock().last().copied();
        let won = self.game.is_won();

        div()
            .key_context("GamesWindow")
            .size_full()
            .on_key_down(widgets::navigate_focus)
            .flex()
            .flex_col()
            .bg(theme::window_bg())
            .border_1()
            .border_color(theme::border_color())
            .child(widgets::title_bar(
                "Solitaire",
                |_, window, _| window.minimize_window(),
                |_, window, _| window.toggle_fullscreen(),
                |_, window, _| window.remove_window(),
            ))
            .child(self.toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .relative()
                    .bg(theme::vertical(0x0c6a47, 0x04341f))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        self.slot(
                                            stock
                                                .map(|_| "assets/games/cards/back.png".to_string()),
                                            false,
                                        )
                                        .cursor_pointer()
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(Self::draw_card),
                                        ),
                                    )
                                    .child(
                                        self.slot(
                                            waste.map(|card| card.asset()),
                                            matches!(self.selected, Some(Pick::Waste)),
                                        )
                                        .cursor_pointer()
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(Self::click_waste),
                                        ),
                                    ),
                            )
                            .child(div().flex_1())
                            .children(
                                Suit::ALL
                                    .into_iter()
                                    .map(|suit| self.foundation(suit, cx).into_any_element()),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .gap_2()
                            .children((0..TABLEAU).map(|column| self.tableau_column(column, cx))),
                    )
                    .children(if won {
                        Some(
                            div()
                                .absolute()
                                .top(px(250.))
                                .left(px(0.))
                                .right(px(0.))
                                .flex()
                                .justify_center()
                                .child(
                                    div()
                                        .px(px(24.))
                                        .py(px(12.))
                                        .rounded_md()
                                        .bg(theme::panel_bg())
                                        .border_1()
                                        .border_color(theme::accent())
                                        .font_family(theme::FONT)
                                        .text_size(px(16.))
                                        .font_weight(theme::bold())
                                        .text_color(theme::text_primary())
                                        .child("You win!"),
                                ),
                        )
                    } else {
                        None
                    }),
            )
            .child(
                div()
                    .min_h(px(40.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::panel_bg())
                    .border_t_1()
                    .border_color(theme::soft_border())
                    .font_family(theme::FONT)
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .child("Click the stock to draw · select a card, then click its destination"),
            )
    }
}
