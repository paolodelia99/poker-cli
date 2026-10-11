use super::EngineError;

/// Rank characters, weakest (`2`) to strongest (`A`); index 0 == rank value 2.
const RANK_CHARS: [char; 13] = [
    '2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A',
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Suit {
    Club,
    Diamond,
    Heart,
    Spade,
}

/// A card rank, `2..=14` where 14 is the Ace. Larger is stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rank(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

fn rank_char(rank: Rank) -> char {
    RANK_CHARS[(rank.0 - 2) as usize]
}

fn rank_from_char(c: char) -> Option<Rank> {
    RANK_CHARS
        .iter()
        .position(|&r| r == c)
        .map(|i| Rank(i as u8 + 2))
}

fn suit_char(suit: Suit) -> char {
    match suit {
        Suit::Club => 'c',
        Suit::Diamond => 'd',
        Suit::Heart => 'h',
        Suit::Spade => 's',
    }
}

fn suit_from_char(c: char) -> Option<Suit> {
    match c {
        'c' => Some(Suit::Club),
        'd' => Some(Suit::Diamond),
        'h' => Some(Suit::Heart),
        's' => Some(Suit::Spade),
        _ => None,
    }
}

fn suit_index(suit: Suit) -> u8 {
    match suit {
        Suit::Club => 0,
        Suit::Diamond => 1,
        Suit::Heart => 2,
        Suit::Spade => 3,
    }
}

impl Card {
    /// Parses `RankSuit` notation: rank in `2..=9TJQKA`, suit in `cdhs`.
    pub fn parse(s: &str) -> Result<Card, EngineError> {
        let invalid = || EngineError::InvalidCardString(s.to_string());
        let mut chars = s.chars();
        let rank_ch = chars.next().ok_or_else(invalid)?;
        let suit_ch = chars.next().ok_or_else(invalid)?;
        if chars.next().is_some() {
            return Err(invalid());
        }
        let rank = rank_from_char(rank_ch).ok_or_else(invalid)?;
        let suit = suit_from_char(suit_ch).ok_or_else(invalid)?;
        Ok(Card { rank, suit })
    }

    /// Inverse of [`Card::parse`] for valid cards: e.g. `Card::display` of
    /// `"As"` yields `"As"`.
    pub fn display(&self) -> String {
        format!("{}{}", rank_char(self.rank), suit_char(self.suit))
    }

    /// Unique, stable bijection of the 52 cards onto `0..52`.
    pub fn index(&self) -> u8 {
        (self.rank.0 - 2) * 4 + suit_index(self.suit)
    }
}

/// Namespace for deck constructors; a `Deck` itself holds no state.
pub struct Deck;

impl Deck {
    /// All 52 cards of a standard deck, ordered by rank ascending then suit.
    pub fn standard() -> [Card; 52] {
        let mut cards = [Card {
            rank: Rank(2),
            suit: Suit::Club,
        }; 52];
        let mut i = 0;
        for rank in 2..=14 {
            for suit in [Suit::Club, Suit::Diamond, Suit::Heart, Suit::Spade] {
                cards[i] = Card {
                    rank: Rank(rank),
                    suit,
                };
                i += 1;
            }
        }
        cards
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineError;

    #[test]
    fn test_parse_display_roundtrip() {
        for s in ["2c", "7d", "Ts", "Jh", "Qs", "Ad"] {
            assert_eq!(Card::parse(s).unwrap().display(), s);
        }
    }

    #[test]
    fn test_parse_rejects_bad_input() {
        assert!(matches!(Card::parse("1c"), Err(EngineError::InvalidCardString(_))));
        assert!(matches!(Card::parse("Xc"), Err(EngineError::InvalidCardString(_))));
        assert!(matches!(Card::parse("Ax"), Err(EngineError::InvalidCardString(_))));
        assert!(matches!(Card::parse(""), Err(EngineError::InvalidCardString(_))));
    }

    #[test]
    fn test_index_is_unique_0_to_51() {
        let mut seen = [false; 52];
        for c in Deck::standard() {
            let i = c.index() as usize;
            assert!(i < 52 && !seen[i]);
            seen[i] = true;
        }
    }
}
