#![allow(dead_code)]

//! Local chess with the complete movement rules: castling, en passant,
//! promotion to queen, check, checkmate and stalemate. Pure board logic so it
//! can be tested without a window.

pub const SIZE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn opposite(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Color::White => "White",
            Color::Black => "Black",
        }
    }

    fn forward(self) -> isize {
        match self {
            Color::White => -1,
            Color::Black => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl Kind {
    pub fn glyph(self) -> &'static str {
        match self {
            Kind::Pawn => "♟",
            Kind::Knight => "♞",
            Kind::Bishop => "♝",
            Kind::Rook => "♜",
            Kind::Queen => "♛",
            Kind::King => "♚",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub color: Color,
    pub kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Castling {
    wk: bool,
    wq: bool,
    bk: bool,
    bq: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Check,
    Checkmate(Color),
    Stalemate,
}

#[derive(Clone)]
struct Snapshot {
    board: [Option<Piece>; 64],
    turn: Color,
    castling: Castling,
    en_passant: Option<usize>,
    halfmove: u32,
    fullmove: u32,
    moves: u32,
}

#[derive(Clone)]
pub struct Game {
    board: [Option<Piece>; 64],
    turn: Color,
    castling: Castling,
    en_passant: Option<usize>,
    halfmove: u32,
    fullmove: u32,
    pub moves: u32,
    history: Vec<Snapshot>,
}

impl Game {
    pub fn new() -> Self {
        let mut board = [None; 64];
        let back = [
            Kind::Rook,
            Kind::Knight,
            Kind::Bishop,
            Kind::Queen,
            Kind::King,
            Kind::Bishop,
            Kind::Knight,
            Kind::Rook,
        ];
        for column in 0..SIZE {
            board[column] = Some(Piece {
                color: Color::Black,
                kind: back[column],
            });
            board[SIZE + column] = Some(Piece {
                color: Color::Black,
                kind: Kind::Pawn,
            });
            board[6 * SIZE + column] = Some(Piece {
                color: Color::White,
                kind: Kind::Pawn,
            });
            board[7 * SIZE + column] = Some(Piece {
                color: Color::White,
                kind: back[column],
            });
        }
        Self {
            board,
            turn: Color::White,
            castling: Castling {
                wk: true,
                wq: true,
                bk: true,
                bq: true,
            },
            en_passant: None,
            halfmove: 0,
            fullmove: 1,
            moves: 0,
            history: Vec::new(),
        }
    }

    pub fn turn(&self) -> Color {
        self.turn
    }

    pub fn piece(&self, square: usize) -> Option<Piece> {
        self.board.get(square).copied().flatten()
    }

    pub fn en_passant(&self) -> Option<usize> {
        self.en_passant
    }

    pub fn status(&self) -> Status {
        let in_check = self.in_check(self.turn);
        if self.legal_moves().is_empty() {
            if in_check {
                Status::Checkmate(self.turn.opposite())
            } else {
                Status::Stalemate
            }
        } else if in_check {
            Status::Check
        } else {
            Status::Ongoing
        }
    }

    pub fn in_check(&self, color: Color) -> bool {
        let Some(king) = self.king_square(color) else {
            return false;
        };
        self.is_attacked(king, color.opposite())
    }

    pub fn king_square(&self, color: Color) -> Option<usize> {
        (0..64).find(|square| {
            self.board[*square]
                .map(|piece| piece.color == color && piece.kind == Kind::King)
                .unwrap_or(false)
        })
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let mut legal = Vec::new();
        for from in 0..64 {
            if self.board[from]
                .map(|piece| piece.color == self.turn)
                .unwrap_or(false)
            {
                for movement in self.pseudo_moves(from) {
                    let mut next = self.clone();
                    next.make_move(movement.from, movement.to);
                    if !next.in_check(self.turn) {
                        legal.push(movement);
                    }
                }
            }
        }
        legal
    }

    pub fn legal_moves_from(&self, from: usize) -> Vec<Move> {
        self.legal_moves()
            .into_iter()
            .filter(|movement| movement.from == from)
            .collect()
    }

    pub fn apply(&mut self, from: usize, to: usize) -> bool {
        if !self
            .legal_moves()
            .iter()
            .any(|movement| movement.from == from && movement.to == to)
        {
            return false;
        }
        self.record();
        self.make_move(from, to);
        self.moves += 1;
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.history.pop() else {
            return false;
        };
        self.board = snapshot.board;
        self.turn = snapshot.turn;
        self.castling = snapshot.castling;
        self.en_passant = snapshot.en_passant;
        self.halfmove = snapshot.halfmove;
        self.fullmove = snapshot.fullmove;
        self.moves = snapshot.moves;
        true
    }

    fn pseudo_moves(&self, from: usize) -> Vec<Move> {
        let Some(piece) = self.board[from] else {
            return Vec::new();
        };
        if piece.color != self.turn {
            return Vec::new();
        }
        let mut moves = Vec::new();
        let (row, column) = (row(from) as isize, (from % SIZE) as isize);

        match piece.kind {
            Kind::Pawn => {
                let forward = piece.color.forward();
                let start_row = if piece.color == Color::White { 6 } else { 1 };
                let one = (row + forward, column);
                if in_board(one.0, one.1) && self.empty(one.0, one.1) {
                    moves.push(movement(from, one));
                    if row as usize == start_row {
                        let two = (row + forward * 2, column);
                        if self.empty(two.0, two.1) {
                            moves.push(movement(from, two));
                        }
                    }
                }
                for dc in [-1, 1] {
                    let target = (row + forward, column + dc);
                    if !in_board(target.0, target.1) {
                        continue;
                    }
                    let target_square = square(target.0 as usize, target.1 as usize);
                    if self.board[target_square]
                        .map(|other| other.color != piece.color)
                        .unwrap_or(false)
                    {
                        moves.push(Move {
                            from,
                            to: target_square,
                        });
                    } else if self.en_passant == Some(target_square)
                        && self
                            .board
                            .get(square(row as usize, (column + dc) as usize))
                            .map(|other| {
                                other
                                    .map(|other| {
                                        other.color != piece.color && other.kind == Kind::Pawn
                                    })
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false)
                    {
                        moves.push(Move {
                            from,
                            to: target_square,
                        });
                    }
                }
            }
            Kind::Knight => {
                for (dr, dc) in [
                    (-2, -1),
                    (-2, 1),
                    (-1, -2),
                    (-1, 2),
                    (1, -2),
                    (1, 2),
                    (2, -1),
                    (2, 1),
                ] {
                    if let Some(target) = step(row, column, dr, dc) {
                        if self.can_land(target, piece.color) {
                            moves.push(Move {
                                from,
                                to: square(target.0 as usize, target.1 as usize),
                            });
                        }
                    }
                }
            }
            Kind::Bishop | Kind::Rook | Kind::Queen => {
                let directions: &[(isize, isize)] = match piece.kind {
                    Kind::Bishop => &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
                    Kind::Rook => &[(-1, 0), (1, 0), (0, -1), (0, 1)],
                    _ => &[
                        (-1, -1),
                        (-1, 1),
                        (1, -1),
                        (1, 1),
                        (-1, 0),
                        (1, 0),
                        (0, -1),
                        (0, 1),
                    ],
                };
                for (dr, dc) in directions {
                    let mut target = (row, column);
                    loop {
                        let Some(next) = step(target.0, target.1, *dr, *dc) else {
                            break;
                        };
                        target = next;
                        let target_square = square(target.0 as usize, target.1 as usize);
                        match self.board[target_square] {
                            None => moves.push(Move {
                                from,
                                to: target_square,
                            }),
                            Some(other) => {
                                if other.color != piece.color {
                                    moves.push(Move {
                                        from,
                                        to: target_square,
                                    });
                                }
                                break;
                            }
                        }
                    }
                }
            }
            Kind::King => {
                for dr in -1..=1 {
                    for dc in -1..=1 {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        if let Some(target) = step(row, column, dr, dc) {
                            if self.can_land(target, piece.color) {
                                moves.push(Move {
                                    from,
                                    to: square(target.0 as usize, target.1 as usize),
                                });
                            }
                        }
                    }
                }
                self.castling_moves(from, piece, &mut moves);
            }
        }
        moves
    }

    fn castling_moves(&self, from: usize, piece: Piece, moves: &mut Vec<Move>) {
        if piece.kind != Kind::King || self.in_check(piece.color) {
            return;
        }
        let (home, king_side, queen_side, rook_king, rook_queen) = match piece.color {
            Color::White => (
                square(7, 4),
                square(7, 6),
                square(7, 2),
                square(7, 7),
                square(7, 0),
            ),
            Color::Black => (
                square(0, 4),
                square(0, 6),
                square(0, 2),
                square(0, 7),
                square(0, 0),
            ),
        };
        if from != home {
            return;
        }
        let rights = self.castling;
        let (king_right, queen_right) = match piece.color {
            Color::White => (rights.wk, rights.wq),
            Color::Black => (rights.bk, rights.bq),
        };
        let enemy = piece.color.opposite();

        if king_right
            && self.board[rook_king]
                .map(|rook| rook.color == piece.color && rook.kind == Kind::Rook)
                .unwrap_or(false)
            && self.board[square(row(from), 5)].is_none()
            && self.board[king_side].is_none()
            && !self.is_attacked(square(row(from), 5), enemy)
            && !self.is_attacked(king_side, enemy)
        {
            moves.push(Move {
                from,
                to: king_side,
            });
        }

        if queen_right
            && self.board[rook_queen]
                .map(|rook| rook.color == piece.color && rook.kind == Kind::Rook)
                .unwrap_or(false)
            && self.board[square(row(from), 1)].is_none()
            && self.board[square(row(from), 2)].is_none()
            && self.board[square(row(from), 3)].is_none()
            && !self.is_attacked(square(row(from), 3), enemy)
            && !self.is_attacked(queen_side, enemy)
        {
            moves.push(Move {
                from,
                to: queen_side,
            });
        }
    }

    fn is_attacked(&self, target: usize, by: Color) -> bool {
        let (row, column) = (row(target) as isize, (target % SIZE) as isize);

        let pawn_row = row + if by == Color::White { 1 } else { -1 };
        for dc in [-1, 1] {
            if in_board(pawn_row, column + dc) {
                let square = square(pawn_row as usize, (column + dc) as usize);
                if self.board[square]
                    .map(|piece| piece.color == by && piece.kind == Kind::Pawn)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }

        for (dr, dc) in [
            (-2, -1),
            (-2, 1),
            (-1, -2),
            (-1, 2),
            (1, -2),
            (1, 2),
            (2, -1),
            (2, 1),
        ] {
            if let Some((r, c)) = step(row, column, dr, dc) {
                let square = square(r as usize, c as usize);
                if self.board[square]
                    .map(|piece| piece.color == by && piece.kind == Kind::Knight)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }

        for dr in -1..=1 {
            for dc in -1..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                if let Some((r, c)) = step(row, column, dr, dc) {
                    let square = square(r as usize, c as usize);
                    if self.board[square]
                        .map(|piece| piece.color == by && piece.kind == Kind::King)
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
        }

        for (directions, kinds) in [
            (
                [(-1, 0), (1, 0), (0, -1), (0, 1)],
                [Kind::Rook, Kind::Queen],
            ),
            (
                [(-1, -1), (-1, 1), (1, -1), (1, 1)],
                [Kind::Bishop, Kind::Queen],
            ),
        ] {
            for (dr, dc) in directions {
                let mut current = (row, column);
                while let Some(next) = step(current.0, current.1, dr, dc) {
                    current = next;
                    let square = square(current.0 as usize, current.1 as usize);
                    if let Some(piece) = self.board[square] {
                        if piece.color == by && kinds.contains(&piece.kind) {
                            return true;
                        }
                        break;
                    }
                }
            }
        }
        false
    }

    fn make_move(&mut self, from: usize, to: usize) {
        let mut piece = self.board[from].take().expect("move starts on a piece");
        let captured = self.board[to];
        let color = piece.color;

        if piece.kind == Kind::Pawn && from % SIZE != to % SIZE && captured.is_none() {
            let captured_square = if color == Color::White { to + SIZE } else { to - SIZE };
            self.board[captured_square] = None;
        }

        if piece.kind == Kind::King && from.abs_diff(to) == 2 {
            let (rook_from, rook_to) = match (color, to % SIZE) {
                (Color::White, 6) => (square(7, 7), square(7, 5)),
                (Color::White, _) => (square(7, 0), square(7, 3)),
                (Color::Black, 6) => (square(0, 7), square(0, 5)),
                (Color::Black, _) => (square(0, 0), square(0, 3)),
            };
            let rook = self.board[rook_from].take();
            self.board[rook_to] = rook;
        }

        if piece.kind == Kind::Pawn && (row(to) == 0 || row(to) == SIZE - 1) {
            piece.kind = Kind::Queen;
        }
        self.board[to] = Some(piece);

        match (color, from) {
            (Color::White, 60) => {
                self.castling.wk = false;
                self.castling.wq = false;
            }
            (Color::Black, 4) => {
                self.castling.bk = false;
                self.castling.bq = false;
            }
            (Color::White, 63) => self.castling.wk = false,
            (Color::White, 56) => self.castling.wq = false,
            (Color::Black, 7) => self.castling.bk = false,
            (Color::Black, 0) => self.castling.bq = false,
            _ => {}
        }
        match to {
            63 => self.castling.wk = false,
            56 => self.castling.wq = false,
            7 => self.castling.bk = false,
            0 => self.castling.bq = false,
            _ => {}
        }

        self.en_passant = if piece.kind == Kind::Pawn && from.abs_diff(to) == 2 * SIZE {
            Some((from + to) / 2)
        } else {
            None
        };
        if piece.kind == Kind::Pawn || captured.is_some() {
            self.halfmove = 0;
        } else {
            self.halfmove += 1;
        }
        if color == Color::Black {
            self.fullmove += 1;
        }
        self.turn = color.opposite();
    }

    fn can_land(&self, target: (isize, isize), color: Color) -> bool {
        let target_square = square(target.0 as usize, target.1 as usize);
        self.board[target_square]
            .map(|piece| piece.color != color)
            .unwrap_or(true)
    }

    fn empty(&self, row: isize, column: isize) -> bool {
        in_board(row, column) && self.board[square(row as usize, column as usize)].is_none()
    }

    fn record(&mut self) {
        self.history.push(Snapshot {
            board: self.board,
            turn: self.turn,
            castling: self.castling,
            en_passant: self.en_passant,
            halfmove: self.halfmove,
            fullmove: self.fullmove,
            moves: self.moves,
        });
    }
}

fn row(square: usize) -> usize {
    square / SIZE
}

fn square(row: usize, column: usize) -> usize {
    row * SIZE + column
}

fn in_board(row: isize, column: isize) -> bool {
    (0..SIZE as isize).contains(&row) && (0..SIZE as isize).contains(&column)
}

fn step(row: isize, column: isize, dr: isize, dc: isize) -> Option<(isize, isize)> {
    let row = row + dr;
    let column = column + dc;
    if in_board(row, column) {
        Some((row, column))
    } else {
        None
    }
}

fn movement(from: usize, target: (isize, isize)) -> Move {
    Move {
        from,
        to: square(target.0 as usize, target.1 as usize),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_with_kings() -> Game {
        let mut game = Game::new();
        game.board = [None; 64];
        game.board[square(7, 4)] = Some(Piece {
            color: Color::White,
            kind: Kind::King,
        });
        game.board[square(0, 4)] = Some(Piece {
            color: Color::Black,
            kind: Kind::King,
        });
        game.castling = Castling::default();
        game.en_passant = None;
        game.turn = Color::White;
        game.history.clear();
        game.moves = 0;
        game
    }

    fn place(game: &mut Game, row: usize, column: usize, color: Color, kind: Kind) {
        game.board[square(row, column)] = Some(Piece { color, kind });
    }

    #[test]
    fn initial_position_has_twenty_legal_moves() {
        let game = Game::new();
        assert_eq!(game.turn(), Color::White);
        assert_eq!(game.legal_moves().len(), 20);
        assert_eq!(game.status(), Status::Ongoing);
    }

    #[test]
    fn castling_moves_the_rook_and_is_blocked_through_check() {
        let mut game = empty_with_kings();
        place(&mut game, 7, 7, Color::White, Kind::Rook);
        game.castling.wk = true;
        let king = square(7, 4);
        assert!(game
            .legal_moves_from(king)
            .iter()
            .any(|movement| movement.to == square(7, 6)));
        assert!(game.apply(king, square(7, 6)));
        assert_eq!(game.piece(square(7, 6)).unwrap().kind, Kind::King);
        assert_eq!(game.piece(square(7, 5)).unwrap().kind, Kind::Rook);
        assert!(game.piece(square(7, 7)).is_none());

        let mut blocked = empty_with_kings();
        place(&mut blocked, 7, 7, Color::White, Kind::Rook);
        place(&mut blocked, 0, 5, Color::Black, Kind::Rook);
        blocked.castling.wk = true;
        assert!(!blocked
            .legal_moves_from(square(7, 4))
            .iter()
            .any(|movement| movement.to == square(7, 6)));
    }

    #[test]
    fn en_passant_removes_the_passed_pawn() {
        let mut game = empty_with_kings();
        place(&mut game, 3, 4, Color::White, Kind::Pawn);
        place(&mut game, 1, 3, Color::Black, Kind::Pawn);
        game.turn = Color::Black;
        assert!(game.apply(square(1, 3), square(3, 3)));
        assert_eq!(game.en_passant(), Some(square(2, 3)));
        game.turn = Color::White;
        assert!(game.apply(square(3, 4), square(2, 3)));
        assert!(game.piece(square(3, 3)).is_none());
        assert_eq!(game.piece(square(2, 3)).unwrap().kind, Kind::Pawn);
    }

    #[test]
    fn promotion_becomes_a_queen() {
        let mut game = empty_with_kings();
        place(&mut game, 1, 0, Color::White, Kind::Pawn);
        assert!(game.apply(square(1, 0), square(0, 0)));
        assert_eq!(game.piece(square(0, 0)).unwrap().kind, Kind::Queen);
    }

    #[test]
    fn checkmate_and_stalemate_are_reported() {
        let mut mate = empty_with_kings();
        mate.board[square(0, 4)] = None;
        place(&mut mate, 0, 6, Color::Black, Kind::King);
        place(&mut mate, 1, 5, Color::Black, Kind::Pawn);
        place(&mut mate, 1, 6, Color::Black, Kind::Pawn);
        place(&mut mate, 1, 7, Color::Black, Kind::Pawn);
        place(&mut mate, 0, 0, Color::White, Kind::Rook);
        mate.turn = Color::Black;
        assert_eq!(mate.status(), Status::Checkmate(Color::White));

        let mut stalemate = empty_with_kings();
        stalemate.board[square(0, 4)] = None;
        stalemate.board[square(7, 4)] = None;
        place(&mut stalemate, 0, 0, Color::Black, Kind::King);
        place(&mut stalemate, 1, 2, Color::White, Kind::Queen);
        place(&mut stalemate, 2, 2, Color::White, Kind::King);
        stalemate.turn = Color::Black;
        assert_eq!(stalemate.status(), Status::Stalemate);
    }

    #[test]
    fn pinned_piece_cannot_open_the_king() {
        let mut game = empty_with_kings();
        place(&mut game, 6, 4, Color::White, Kind::Rook);
        place(&mut game, 0, 4, Color::Black, Kind::Rook);
        let moves = game.legal_moves_from(square(6, 4));
        assert!(!moves.iter().any(|movement| movement.to == square(6, 3)));
        assert!(moves.iter().any(|movement| movement.to == square(5, 4)));
    }

    #[test]
    fn undo_restores_the_position() {
        let mut game = Game::new();
        assert!(game.apply(square(6, 4), square(4, 4)));
        assert!(game.undo());
        assert_eq!(game.piece(square(6, 4)).unwrap().kind, Kind::Pawn);
        assert!(game.piece(square(4, 4)).is_none());
        assert_eq!(game.turn(), Color::White);
    }
}
