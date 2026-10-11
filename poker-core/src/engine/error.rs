use crate::engine::Card;

/// Errors returned by the engine for invalid inputs or illegal requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// A card string was not two chars in `RankSuit` notation (e.g. `"As"`).
    InvalidCardString(String),
    /// A board must hold exactly 5 cards to be complete.
    InvalidBoardSize(usize),
    /// The same card appeared more than once in a single hand.
    DuplicateCard(Card),
    /// An equity calculation needs at least one opponent.
    NoOpponents,
    /// Not enough undealt cards remain to satisfy a request.
    NotEnoughCards { need: usize, have: usize },
    /// An iteration count must be greater than zero.
    InvalidIterations(usize),
    /// A seed string could not be interpreted.
    InvalidSeed(String),
    /// The requested number of scenarios exceeds the supported maximum.
    TooManyScenarios { scenarios: u64 },
}
