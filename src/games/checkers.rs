#![allow(dead_code)]

//! English draughts (checkers) for two local players. The rules are pure Rust
//! so the board logic can be tested without GPUI. Black moves down, red moves
//! up; captures are mandatory and a jump chain keeps the turn.

pub const SIZE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Player {
    Black,
    Red,
}

impl Player {
    pub fn opponent(self) -> Self {
        match self {
            Player::Black => Player::Red,
            Player::Red => Player::Black,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Player::Black => "Black",
            Player::Red => "Red",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub player: Player,
    pub king: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move {
    pub from: usize,
    pub to: usize,
    pub captured: Option<usize>,
}

#[derive(Clone)]
struct Snapshot {
    board: [Option<Piece>; 64],
    turn: Player,
    chain: Option<usize>,
    moves: u32,
}

#[derive(Clone)]
pub struct Game {
    board: [Option<Piece>; 64],
    turn: Player,
    chain: Option<usize>,
    pub moves: u32,
    history: Vec<Snapshot>,
}

impl Game {
    pub fn new() -> Self {
        let mut board = [None; 64];
        for row in 0..3 {
            for column in 0..SIZE {
                if dark_square(row, column) {
                    board[row * SIZE + column] = Some(Piece {
                        player: Player::Black,
                        king: false,
                    });
                }
            }
        }
        for row in 5..8 {
            for column in 0..SIZE {
                if dark_square(row, column) {
                    board[row * SIZE + column] = Some(Piece {
                        player: Player::Red,
                        king: false,
                    });
                }
            }
        }
        Self {
            board,
            turn: Player::Black,
            chain: None,
            moves: 0,
            history: Vec::new(),
        }
    }

    pub fn turn(&self) -> Player {
        self.turn
    }

    /// The square that must continue a multi-jump, if any.
    pub fn chain(&self) -> Option<usize> {
        self.chain
    }

    pub fn piece(&self, square: usize) -> Option<Piece> {
        self.board.get(square).copied().flatten()
    }

    pub fn count(&self, player: Player) -> usize {
        self.board
            .iter()
            .filter(|piece| piece.map(|piece| piece.player) == Some(player))
            .count()
    }

    pub fn winner(&self) -> Option<Player> {
        if self.count(self.turn.opponent()) == 0 {
            return Some(self.turn);
        }
        if self.legal_moves().is_empty() {
            return Some(self.turn.opponent());
        }
        None
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let sources: Vec<usize> = match self.chain {
            Some(square) => vec![square],
            None => (0..64)
                .filter(|square| {
                    self.board[*square]
                        .map(|piece| piece.player == self.turn)
                        .unwrap_or(false)
                })
                .collect(),
        };

        let mut moves: Vec<Move> = sources
            .into_iter()
            .flat_map(|square| self.moves_from_square(square, false))
            .collect();
        if moves.iter().any(|movement| movement.captured.is_some()) {
            moves.retain(|movement| movement.captured.is_some());
        }
        moves
    }

    pub fn legal_moves_from(&self, square: usize) -> Vec<Move> {
        self.legal_moves()
            .into_iter()
            .filter(|movement| movement.from == square)
            .collect()
    }

    pub fn apply(&mut self, from: usize, to: usize) -> bool {
        let Some(movement) = self
            .legal_moves()
            .into_iter()
            .find(|movement| movement.from == from && movement.to == to)
        else {
            return false;
        };

        self.record();
        let mut piece = self.board[from].take().expect("legal move starts on a piece");
        if let Some(captured) = movement.captured {
            self.board[captured] = None;
        }
        let promoted = !piece.king
            && ((piece.player == Player::Black && row(to) == SIZE - 1)
                || (piece.player == Player::Red && row(to) == 0));
        if promoted {
            piece.king = true;
        }
        self.board[to] = Some(piece);
        self.moves += 1;

        if movement.captured.is_some() && !promoted {
            let more = self.moves_from_square(to, true);
            if !more.is_empty() {
                self.chain = Some(to);
                return true;
            }
        }
        self.chain = None;
        self.turn = self.turn.opponent();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.history.pop() else {
            return false;
        };
        self.board = snapshot.board;
        self.turn = snapshot.turn;
        self.chain = snapshot.chain;
        self.moves = snapshot.moves;
        true
    }

    fn moves_from_square(&self, square: usize, captures_only: bool) -> Vec<Move> {
        let Some(piece) = self.board.get(square).copied().flatten() else {
            return Vec::new();
        };
        let directions: [(isize, isize); 4] = if piece.king {
            [(-1, -1), (-1, 1), (1, -1), (1, 1)]
        } else if piece.player == Player::Black {
            [(1, -1), (1, 1), (0, 0), (0, 0)]
        } else {
            [(-1, -1), (-1, 1), (0, 0), (0, 0)]
        };

        let mut moves = Vec::new();
        for (dr, dc) in directions {
            if dr == 0 && dc == 0 {
                continue;
            }
            let Some((next_row, next_col)) = offset(square, dr, dc) else {
                continue;
            };
            let next = next_row * SIZE + next_col;
            match self.board[next] {
                None if !captures_only => moves.push(Move {
                    from: square,
                    to: next,
                    captured: None,
                }),
                Some(target) if target.player != piece.player => {
                    let Some((landing_row, landing_col)) = offset(next, dr, dc) else {
                        continue;
                    };
                    let landing = landing_row * SIZE + landing_col;
                    if self.board[landing].is_none() {
                        moves.push(Move {
                            from: square,
                            to: landing,
                            captured: Some(next),
                        });
                    }
                }
                _ => {}
            }
        }
        moves
    }

    fn record(&mut self) {
        self.history.push(Snapshot {
            board: self.board,
            turn: self.turn,
            chain: self.chain,
            moves: self.moves,
        });
    }
}

fn dark_square(row: usize, column: usize) -> bool {
    (row + column) % 2 == 1
}

fn row(square: usize) -> usize {
    square / SIZE
}

fn offset(square: usize, dr: isize, dc: isize) -> Option<(usize, usize)> {
    let row = row(square) as isize + dr;
    let column = (square % SIZE) as isize + dc;
    if (0..SIZE as isize).contains(&row) && (0..SIZE as isize).contains(&column) {
        Some((row as usize, column as usize))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(row: usize, column: usize) -> usize {
        row * SIZE + column
    }

    fn empty_game() -> Game {
        let mut game = Game::new();
        game.board = [None; 64];
        game.chain = None;
        game.turn = Player::Black;
        game.history.clear();
        game.moves = 0;
        game
    }

    fn place(game: &mut Game, row: usize, column: usize, player: Player, king: bool) {
        game.board[square(row, column)] = Some(Piece { player, king });
    }

    #[test]
    fn starts_with_twelve_pieces_each_on_dark_squares() {
        let game = Game::new();
        assert_eq!(game.count(Player::Black), 12);
        assert_eq!(game.count(Player::Red), 12);
        assert_eq!(game.turn(), Player::Black);
        for index in 0..64 {
            if let Some(piece) = game.piece(index) {
                assert!(dark_square(row(index), index % SIZE));
                assert!(!piece.king);
            }
        }
        assert!(game.piece(square(0, 1)).is_some());
        assert!(game.piece(square(0, 0)).is_none());
    }

    #[test]
    fn men_move_forward_diagonally_only() {
        let mut game = empty_game();
        place(&mut game, 2, 1, Player::Black, false);
        let targets: Vec<usize> = game
            .legal_moves()
            .into_iter()
            .map(|movement| movement.to)
            .collect();
        assert!(targets.contains(&square(3, 0)));
        assert!(targets.contains(&square(3, 2)));
        assert!(!targets.contains(&square(1, 0)));
        assert!(!targets.contains(&square(1, 2)));
    }

    #[test]
    fn kings_move_backwards() {
        let mut game = empty_game();
        place(&mut game, 3, 3, Player::Black, true);
        let targets: Vec<usize> = game
            .legal_moves()
            .into_iter()
            .map(|movement| movement.to)
            .collect();
        assert!(targets.contains(&square(2, 2)));
        assert!(targets.contains(&square(4, 4)));
    }

    #[test]
    fn captures_are_mandatory_and_remove_the_piece() {
        let mut game = empty_game();
        place(&mut game, 2, 1, Player::Black, false);
        place(&mut game, 3, 2, Player::Red, false);
        place(&mut game, 2, 5, Player::Black, false);

        let moves = game.legal_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].to, square(4, 3));
        assert_eq!(moves[0].captured, Some(square(3, 2)));
        assert!(game.apply(square(2, 1), square(4, 3)));
        assert!(game.piece(square(3, 2)).is_none());
        assert_eq!(game.turn(), Player::Red);
    }

    #[test]
    fn a_double_jump_keeps_the_turn_and_chains() {
        let mut game = empty_game();
        place(&mut game, 2, 1, Player::Black, false);
        place(&mut game, 3, 2, Player::Red, false);
        place(&mut game, 5, 4, Player::Red, false);

        assert!(game.apply(square(2, 1), square(4, 3)));
        assert_eq!(game.turn(), Player::Black);
        assert_eq!(game.chain(), Some(square(4, 3)));
        assert_eq!(game.legal_moves().len(), 1);

        assert!(game.apply(square(4, 3), square(6, 5)));
        assert_eq!(game.turn(), Player::Red);
        assert_eq!(game.chain(), None);
        assert_eq!(game.count(Player::Red), 0);
    }

    #[test]
    fn reaching_the_last_row_makes_a_king() {
        let mut game = empty_game();
        place(&mut game, 6, 1, Player::Black, false);
        assert!(game.apply(square(6, 1), square(7, 0)));
        assert!(game.piece(square(7, 0)).unwrap().king);
    }

    #[test]
    fn captures_the_last_piece_to_win() {
        let mut game = empty_game();
        place(&mut game, 2, 1, Player::Black, false);
        place(&mut game, 3, 2, Player::Red, false);
        assert!(game.apply(square(2, 1), square(4, 3)));
        assert_eq!(game.winner(), Some(Player::Black));
    }

    #[test]
    fn undo_restores_the_board_and_turn() {
        let mut game = empty_game();
        place(&mut game, 2, 1, Player::Black, false);
        assert!(game.apply(square(2, 1), square(3, 0)));
        assert!(game.undo());
        assert!(game.piece(square(2, 1)).is_some());
        assert!(game.piece(square(3, 0)).is_none());
        assert_eq!(game.turn(), Player::Black);
    }
}
