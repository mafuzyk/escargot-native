#![allow(dead_code)]

//! Klondike solitaire, the single-player version of the card table MSN shipped
//! with Solitaire Showdown. The rules live here, free of GPUI, so they can be
//! tested without a window.

pub const TABLEAU: usize = 7;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub fn index(self) -> usize {
        match self {
            Suit::Clubs => 0,
            Suit::Diamonds => 1,
            Suit::Hearts => 2,
            Suit::Spades => 3,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Suit::Clubs => "clubs",
            Suit::Diamonds => "diamonds",
            Suit::Hearts => "hearts",
            Suit::Spades => "spades",
        }
    }

    pub fn is_red(self) -> bool {
        matches!(self, Suit::Diamonds | Suit::Hearts)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Card {
    pub suit: Suit,
    pub rank: u8,
}

impl Card {
    /// Path of the CC0 card face shipped under `assets/games/cards`.
    pub fn asset(self) -> String {
        let rank = match self.rank {
            1 => "a".to_string(),
            11 => "j".to_string(),
            12 => "q".to_string(),
            13 => "k".to_string(),
            rank => format!("{rank:02}"),
        };
        format!("assets/games/cards/{}_{rank}.png", self.suit.code())
    }

    pub fn label(self) -> String {
        let rank = match self.rank {
            1 => "A".to_string(),
            11 => "J".to_string(),
            12 => "Q".to_string(),
            13 => "K".to_string(),
            rank => rank.to_string(),
        };
        format!("{rank}{}", match self.suit {
            Suit::Clubs => "♣",
            Suit::Diamonds => "♦",
            Suit::Hearts => "♥",
            Suit::Spades => "♠",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PileCard {
    pub card: Card,
    pub face_up: bool,
}

#[derive(Clone)]
struct Snapshot {
    stock: Vec<Card>,
    waste: Vec<Card>,
    foundations: [Vec<Card>; 4],
    tableau: [Vec<PileCard>; TABLEAU],
    score: i32,
    moves: u32,
}

#[derive(Clone)]
pub struct Game {
    stock: Vec<Card>,
    waste: Vec<Card>,
    foundations: [Vec<Card>; 4],
    tableau: [Vec<PileCard>; TABLEAU],
    pub score: i32,
    pub moves: u32,
    history: Vec<Snapshot>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut deck = Vec::with_capacity(52);
        for suit in Suit::ALL {
            for rank in 1..=13 {
                deck.push(Card { suit, rank });
            }
        }
        shuffle(&mut deck, seed);

        let mut tableau: [Vec<PileCard>; TABLEAU] = Default::default();
        let mut index = 0;
        for (column, pile) in tableau.iter_mut().enumerate() {
            for row in 0..=column {
                pile.push(PileCard {
                    card: deck[index],
                    face_up: row == column,
                });
                index += 1;
            }
        }

        Self {
            stock: deck[index..].to_vec(),
            waste: Vec::new(),
            foundations: Default::default(),
            tableau,
            score: 0,
            moves: 0,
            history: Vec::new(),
        }
    }

    pub fn stock(&self) -> &[Card] {
        &self.stock
    }

    pub fn waste(&self) -> &[Card] {
        &self.waste
    }

    pub fn waste_top(&self) -> Option<Card> {
        self.waste.last().copied()
    }

    pub fn foundation(&self, suit: Suit) -> &[Card] {
        &self.foundations[suit.index()]
    }

    pub fn tableau(&self, column: usize) -> &[PileCard] {
        &self.tableau[column]
    }

    pub fn is_won(&self) -> bool {
        self.foundations.iter().all(|pile| pile.len() == 13)
    }

    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn draw(&mut self) {
        self.record();
        if let Some(card) = self.stock.pop() {
            self.waste.push(card);
            self.moves += 1;
            return;
        }
        if self.waste.is_empty() {
            self.history.pop();
            return;
        }
        while let Some(card) = self.waste.pop() {
            self.stock.push(card);
        }
        self.moves += 1;
    }

    pub fn foundation_accepts(&self, card: Card) -> bool {
        let pile = &self.foundations[card.suit.index()];
        match pile.last() {
            Some(top) => card.rank == top.rank + 1,
            None => card.rank == 1,
        }
    }

    pub fn tableau_accepts(&self, card: Card, column: usize) -> bool {
        match self.tableau[column].last() {
            Some(top) => {
                top.face_up
                    && top.card.rank == card.rank + 1
                    && top.card.suit.is_red() != card.suit.is_red()
            }
            None => card.rank == 13,
        }
    }

    pub fn move_waste_to_foundation(&mut self) -> bool {
        let Some(card) = self.waste_top() else {
            return false;
        };
        if !self.foundation_accepts(card) {
            return false;
        }
        self.record();
        self.waste.pop();
        self.foundations[card.suit.index()].push(card);
        self.score += 10;
        self.moves += 1;
        true
    }

    pub fn move_waste_to_tableau(&mut self, column: usize) -> bool {
        let Some(card) = self.waste_top() else {
            return false;
        };
        if !self.tableau_accepts(card, column) {
            return false;
        }
        self.record();
        self.waste.pop();
        self.tableau[column].push(PileCard {
            card,
            face_up: true,
        });
        self.score += 5;
        self.moves += 1;
        true
    }

    pub fn move_tableau_to_foundation(&mut self, column: usize) -> bool {
        let Some(top) = self.tableau[column].last() else {
            return false;
        };
        if !top.face_up || !self.foundation_accepts(top.card) {
            return false;
        }
        let card = top.card;
        self.record();
        self.tableau[column].pop();
        self.foundations[card.suit.index()].push(card);
        self.score += 10;
        self.moves += 1;
        self.flip_top(column);
        true
    }

    /// Moves the face-up run that starts at `index` from `from` onto `to`.
    pub fn move_run(&mut self, from: usize, index: usize, to: usize) -> bool {
        if from == to || index >= self.tableau[from].len() {
            return false;
        }
        let run = &self.tableau[from][index..];
        if !run.iter().all(|pile| pile.face_up) {
            return false;
        }
        for pair in run.windows(2) {
            let upper = pair[0].card;
            let lower = pair[1].card;
            if upper.rank != lower.rank + 1 || upper.suit.is_red() == lower.suit.is_red() {
                return false;
            }
        }
        let first = run[0].card;
        if !self.tableau_accepts(first, to) {
            return false;
        }

        self.record();
        let run: Vec<PileCard> = self.tableau[from].drain(index..).collect();
        self.tableau[to].extend(run);
        self.moves += 1;
        self.flip_top(from);
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.history.pop() else {
            return false;
        };
        self.stock = snapshot.stock;
        self.waste = snapshot.waste;
        self.foundations = snapshot.foundations;
        self.tableau = snapshot.tableau;
        self.score = snapshot.score;
        self.moves = snapshot.moves;
        true
    }

    fn record(&mut self) {
        self.history.push(Snapshot {
            stock: self.stock.clone(),
            waste: self.waste.clone(),
            foundations: self.foundations.clone(),
            tableau: self.tableau.clone(),
            score: self.score,
            moves: self.moves,
        });
    }

    fn flip_top(&mut self, column: usize) {
        if let Some(top) = self.tableau[column].last_mut() {
            if !top.face_up {
                top.face_up = true;
                self.score += 5;
            }
        }
    }
}

fn shuffle(deck: &mut [Card], seed: u64) {
    let mut state = if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed };
    for index in (1..deck.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let swap = (state % (index as u64 + 1)) as usize;
        deck.swap(index, swap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: u8, suit: Suit) -> Card {
        Card { suit, rank }
    }

    fn up(rank: u8, suit: Suit) -> PileCard {
        PileCard {
            card: card(rank, suit),
            face_up: true,
        }
    }

    #[test]
    fn deal_uses_all_cards_and_the_klondike_layout() {
        let game = Game::new(42);
        let total = game.stock().len()
            + game.waste().len()
            + game.tableau.iter().map(Vec::len).sum::<usize>()
            + game.foundations.iter().map(Vec::len).sum::<usize>();
        assert_eq!(total, 52);
        assert_eq!(game.stock().len(), 24);
        for column in 0..TABLEAU {
            assert_eq!(game.tableau(column).len(), column + 1);
            assert!(game.tableau(column).last().unwrap().face_up);
            assert!(game.tableau(column)[..column].iter().all(|pile| !pile.face_up));
        }
    }

    #[test]
    fn foundations_start_at_ace_and_grow_by_suit() {
        let mut game = Game::new(7);
        assert!(game.foundation_accepts(card(1, Suit::Hearts)));
        assert!(!game.foundation_accepts(card(2, Suit::Hearts)));
        game.foundations[Suit::Hearts.index()].push(card(1, Suit::Hearts));
        assert!(game.foundation_accepts(card(2, Suit::Hearts)));
        assert!(!game.foundation_accepts(card(3, Suit::Hearts)));
        assert!(!game.foundation_accepts(card(2, Suit::Clubs)));
    }

    #[test]
    fn tableau_needs_descending_alternating_colours_and_a_king_on_empty() {
        let mut game = Game::new(9);
        for pile in game.tableau.iter_mut() {
            pile.clear();
        }
        game.tableau[0].push(up(7, Suit::Spades));
        assert!(game.tableau_accepts(card(6, Suit::Hearts), 0));
        assert!(!game.tableau_accepts(card(6, Suit::Clubs), 0));
        assert!(!game.tableau_accepts(card(5, Suit::Hearts), 0));
        assert!(!game.tableau_accepts(card(6, Suit::Hearts), 1));
        assert!(game.tableau_accepts(card(13, Suit::Diamonds), 1));
        game.tableau[1].push(PileCard {
            card: card(7, Suit::Spades),
            face_up: false,
        });
        assert!(!game.tableau_accepts(card(6, Suit::Hearts), 1));
    }

    #[test]
    fn moves_a_run_and_flips_the_exposed_card() {
        let mut game = Game::new(11);
        for pile in game.tableau.iter_mut() {
            pile.clear();
        }
        game.tableau[0] = vec![
            PileCard {
                card: card(4, Suit::Clubs),
                face_up: false,
            },
            up(7, Suit::Hearts),
            up(6, Suit::Spades),
        ];
        game.tableau[1] = vec![up(8, Suit::Clubs)];

        assert!(game.move_run(0, 1, 1));
        assert_eq!(game.tableau(0).len(), 1);
        assert!(game.tableau(0)[0].face_up, "exposed card should turn over");
        assert_eq!(game.tableau(1).len(), 3);
        assert_eq!(game.tableau(1)[1].card, card(7, Suit::Hearts));
        assert_eq!(game.tableau(1)[2].card, card(6, Suit::Spades));
        assert_eq!(game.score, 5);
    }

    #[test]
    fn draw_recycles_the_waste_and_undo_restores_state() {
        let mut game = Game::new(5);
        let stock = game.stock().len();
        game.draw();
        assert_eq!(game.stock().len(), stock - 1);
        assert_eq!(game.waste().len(), 1);
        assert!(game.undo());
        assert_eq!(game.stock().len(), stock);
        assert!(game.waste().is_empty());

        while !game.stock().is_empty() {
            game.draw();
        }
        let waste = game.waste().len();
        assert_eq!(waste, stock);
        game.draw();
        assert_eq!(game.stock().len(), waste);
        assert!(game.waste().is_empty());
    }

    #[test]
    fn wins_when_all_foundations_reach_king() {
        let mut game = Game::new(3);
        for suit in Suit::ALL {
            for rank in 1..=13 {
                game.foundations[suit.index()].push(card(rank, suit));
            }
        }
        assert!(game.is_won());
    }
}
