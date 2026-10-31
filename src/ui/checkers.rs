//! Offline checkers (English draughts) window. The board and pieces are drawn
//! with GPUI surfaces; no external art is needed beyond the app chrome.

use gpui::{
    App, Bounds, Context, Div, MouseButton, Render, Window, WindowBounds, WindowDecorations,
    WindowOptions, div, img, prelude::*, rgba, size,
};
use crate::ui::scale::px;

use crate::games::checkers::{Game, Player, SIZE};
use crate::ui::{theme, widgets};

const SQUARE: f32 = 60.;
const BOARD: f32 = SQUARE * SIZE as f32;
const PIECE: f32 = 46.;

pub struct CheckersView {
    game: Game,
    selected: Option<usize>,
}

pub fn open_checkers_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(gpui::px(548.), gpui::px(604.)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("escargot-native-checkers".into()),
            window_decorations: Some(WindowDecorations::Client),
            is_resizable: true,
            window_min_size: Some(size(gpui::px(520.), gpui::px(560.))),
            ..Default::default()
        },
        |_, cx| cx.new(CheckersView::new),
    );
    if let Ok(handle) = handle {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}

impl CheckersView {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            game: Game::new(),
            selected: None,
        }
    }

    fn new_game(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.game = Game::new();
        self.selected = None;
        cx.notify();
    }

    fn undo(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.game.undo() {
            self.selected = None;
        }
        cx.notify();
    }

    fn click_square(&mut self, square: usize, cx: &mut Context<Self>) {
        if self.game.winner().is_some() {
            return;
        }
        if let Some(from) = self.selected {
            if from == square {
                self.selected = None;
                cx.notify();
                return;
            }
            if self.game.apply(from, square) {
                self.selected = self.game.chain();
                cx.notify();
                return;
            }
        }
        self.selected = self
            .game
            .piece(square)
            .filter(|piece| piece.player == self.game.turn())
            .filter(|_| !self.game.legal_moves_from(square).is_empty())
            .map(|_| square);
        cx.notify();
    }

    fn square_color(&self, row: usize, column: usize) -> gpui::Hsla {
        if (row + column) % 2 == 1 {
            if theme::is_night() {
                rgba(0x1d3b2bff).into()
            } else {
                rgba(0x3d6b4fff).into()
            }
        } else if theme::is_night() {
            rgba(0x8b7b5aff).into()
        } else {
            rgba(0xd8c49aff).into()
        }
    }

    fn piece(&self, square: usize) -> impl IntoElement {
        let piece = self.game.piece(square).expect("piece exists");
        let (outer, top, bottom) = match piece.player {
            Player::Black => (0x111111ff, 0x4a4a4a, 0x181818),
            Player::Red => (0x5e120cff, 0xe05a4a, 0x8e1e14),
        };
        let mut disc = div()
            .w(px(PIECE))
            .h(px(PIECE))
            .rounded_full()
            .bg(theme::vertical(top, bottom))
            .border_2()
            .border_color(rgba(outer))
            .shadow_sm()
            .flex()
            .items_center()
            .justify_center();
        if piece.king {
            disc = disc.child(
                div()
                    .font_family(theme::FONT)
                    .text_size(px(14.))
                    .font_weight(theme::bold())
                    .text_color(rgba(0xf4d789ff))
                    .child("K"),
            );
        }
        disc
    }

    fn cell(&self, row: usize, column: usize, targets: &[usize], cx: &mut Context<Self>) -> Div {
        let square = row * SIZE + column;
        let selected = self.selected == Some(square);
        let target = targets.contains(&square);
        let mut cell = div()
            .w(px(SQUARE))
            .h(px(SQUARE))
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .bg(self.square_color(row, column))
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.click_square(square, cx)),
            );
        if selected {
            cell = cell.border_2().border_color(theme::accent());
        }
        if self.game.piece(square).is_some() {
            cell = cell.child(self.piece(square));
        } else if target {
            cell = cell.child(
                div()
                    .w(px(14.))
                    .h(px(14.))
                    .rounded_full()
                    .bg(rgba(0xffffff99)),
            );
        }
        cell
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let (turn, winner) = (self.game.turn(), self.game.winner());
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
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w(px(9.))
                            .h(px(9.))
                            .rounded_full()
                            .border_1()
                            .border_color(rgba(0x9fb3bbff))
                            .bg(match turn {
                                Player::Black => rgba(0x2a2a2aff),
                                Player::Red => rgba(0xc0392bff),
                            }),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT)
                            .text_size(px(12.))
                            .text_color(theme::text_primary())
                            .child(if let Some(winner) = winner {
                                format!("{} wins", winner.label())
                            } else {
                                format!("{} to move", turn.label())
                            }),
                    ),
            )
    }

    fn toolbar_button(
        &self,
        label: &'static str,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        widgets::button(label).on_click(handler)
    }
}

impl Render for CheckersView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::scale::apply(window);
        let targets: Vec<usize> = self
            .selected
            .map(|square| {
                self.game
                    .legal_moves_from(square)
                    .into_iter()
                    .map(|movement| movement.to)
                    .collect()
            })
            .unwrap_or_default();
        let winner = self.game.winner();

        div()
            .key_context("CheckersWindow")
            .size_full()
            .on_key_down(widgets::navigate_focus)
            .flex()
            .flex_col()
            .bg(theme::window_bg())
            .border_1()
            .border_color(theme::border_color())
            .child(widgets::title_bar(
                "Checkers",
                |_, window, _| window.minimize_window(),
                |_, window, _| window.toggle_fullscreen(),
                |_, window, _| window.remove_window(),
            ))
            .child(self.toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .p(px(10.))
                    .bg(theme::panel_gradient())
                    .child(
                        div()
                            .relative()
                            .w(px(BOARD))
                            .h(px(BOARD))
                            .border_2()
                            .border_color(theme::border_color())
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .children((0..SIZE).map(|row| {
                                div().flex().children((0..SIZE).map(|column| {
                                    self.cell(row, column, &targets, cx).into_any_element()
                                }))
                            })),
                    )
                    .children(if winner.is_some() {
                        Some(
                            div()
                                .px(px(14.))
                                .py(px(5.))
                                .rounded_sm()
                                .bg(theme::panel_bg())
                                .border_1()
                                .border_color(theme::accent())
                                .font_family(theme::FONT)
                                .text_size(px(13.))
                                .font_weight(theme::bold())
                                .text_color(theme::text_primary())
                                .child(format!("{} wins. New game?", winner.unwrap().label())),
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
                    .gap_2()
                    .bg(theme::panel_bg())
                    .border_t_1()
                    .border_color(theme::soft_border())
                    .font_family(theme::FONT)
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .child("Captures are mandatory")
                    .child(img("assets/img/check.png").w(px(9.)).h(px(9.)).opacity(0.7))
                    .child("Click a piece, then a highlighted square"),
            )
    }
}
