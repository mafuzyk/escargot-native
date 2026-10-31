//! Offline local chess window. Pieces use the DejaVu Sans chess glyphs on
//! contrasting badges, so no new bitmap assets are needed.

use gpui::{
    App, Bounds, Context, Div, MouseButton, Render, Window, WindowBounds, WindowDecorations,
    WindowOptions, div, prelude::*, rgba, size,
};
use crate::ui::scale::px;

use crate::games::chess::{Color, Game, SIZE, Status};
use crate::ui::{theme, widgets};

const SQUARE: f32 = 64.;
const BOARD: f32 = SQUARE * SIZE as f32;
const BADGE: f32 = 48.;

pub struct ChessView {
    game: Game,
    selected: Option<usize>,
}

pub fn open_chess_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(gpui::px(560.), gpui::px(636.)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("escargot-native-chess".into()),
            window_decorations: Some(WindowDecorations::Client),
            is_resizable: true,
            window_min_size: Some(size(gpui::px(540.), gpui::px(600.))),
            ..Default::default()
        },
        |_, cx| cx.new(ChessView::new),
    );
    if let Ok(handle) = handle {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}

impl ChessView {
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
        if matches!(self.game.status(), Status::Checkmate(_) | Status::Stalemate) {
            return;
        }
        if let Some(from) = self.selected {
            if from == square {
                self.selected = None;
                cx.notify();
                return;
            }
            if self.game.apply(from, square) {
                self.selected = None;
                cx.notify();
                return;
            }
        }
        self.selected = self
            .game
            .piece(square)
            .filter(|piece| piece.color == self.game.turn())
            .filter(|_| !self.game.legal_moves_from(square).is_empty())
            .map(|_| square);
        cx.notify();
    }

    fn square_color(&self, row: usize, column: usize) -> gpui::Hsla {
        if (row + column) % 2 == 1 {
            if theme::is_night() {
                rgba(0x5c4030ff).into()
            } else {
                rgba(0xa97a55ff).into()
            }
        } else if theme::is_night() {
            rgba(0xcbb894ff).into()
        } else {
            rgba(0xf0d9b5ff).into()
        }
    }

    fn piece_badge(&self, square: usize) -> Div {
        let piece = self.game.piece(square).expect("piece exists");
        let (background, color, border) = match piece.color {
            Color::White => (0x3a2b1fcc, 0xfff7e8ff, 0xfff7e8cc),
            Color::Black => (0xf5e6cecc, 0x151515ff, 0x151515ff),
        };
        div()
            .w(px(BADGE))
            .h(px(BADGE))
            .rounded_full()
            .bg(rgba(background))
            .border_1()
            .border_color(rgba(border))
            .shadow_sm()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .font_family("DejaVu Sans")
                    .text_size(px(32.))
                    .text_color(rgba(color))
                    .child(piece.kind.glyph()),
            )
    }

    fn cell(&self, row: usize, column: usize, targets: &[usize], cx: &mut Context<Self>) -> Div {
        let square = row * SIZE + column;
        let selected = self.selected == Some(square);
        let target = targets.contains(&square);
        let in_check = self.game.in_check(self.game.turn())
            && self.game.king_square(self.game.turn()) == Some(square);
        let occupied = self.game.piece(square).is_some();

        let mut cell = div()
            .relative()
            .w(px(SQUARE))
            .h(px(SQUARE))
            .flex()
            .items_center()
            .justify_center()
            .bg(self.square_color(row, column))
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.click_square(square, cx)),
            );

        if in_check {
            cell = cell.border_2().border_color(rgba(0xd13b3bff));
        } else if selected {
            cell = cell.border_2().border_color(theme::accent());
        }

        if occupied {
            cell = cell.child(self.piece_badge(square));
        } else if target {
            cell = cell.child(
                div()
                    .w(px(16.))
                    .h(px(16.))
                    .rounded_full()
                    .bg(rgba(0xfff2c4ff))
                    .border_1()
                    .border_color(rgba(0x1d4d2bcc)),
            );
        }

        if target && occupied {
            cell = cell.child(
                div()
                    .absolute()
                    .top(px(1.))
                    .left(px(1.))
                    .right(px(1.))
                    .bottom(px(1.))
                    .border_2()
                    .border_color(rgba(0xd13b3bff))
                    .rounded_sm(),
            );
        }
        cell
    }

    fn status_text(&self) -> String {
        match self.game.status() {
            Status::Ongoing => format!("{} to move", self.game.turn().label()),
            Status::Check => format!("{} in check", self.game.turn().label()),
            Status::Checkmate(winner) => format!("Checkmate - {} wins", winner.label()),
            Status::Stalemate => "Stalemate - draw".to_string(),
        }
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
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
                    .text_color(theme::text_primary())
                    .child(self.status_text()),
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

impl Render for ChessView {
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
        let finished = matches!(self.game.status(), Status::Checkmate(_) | Status::Stalemate);

        div()
            .key_context("ChessWindow")
            .size_full()
            .on_key_down(widgets::navigate_focus)
            .flex()
            .flex_col()
            .bg(theme::window_bg())
            .border_1()
            .border_color(theme::border_color())
            .child(widgets::title_bar(
                "Chess",
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
                    .children(if finished {
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
                                .child(self.status_text()),
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
                    .child("Click a piece, then a highlighted square. Pawns promote to queens."),
            )
    }
}
