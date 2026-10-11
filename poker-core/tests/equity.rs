use poker_core::engine::{
    equity, CactusKev, Card, Deck, Equity, EquityInput, EquityMode, EquityParams, HandEvaluator,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_public_surface() {
    let c = Card::parse("As").unwrap();
    let d = Deck::standard();
    let e: Equity = equity(
        EquityInput {
            hero: [c, Card::parse("Ah").unwrap()],
            board: vec![],
            opponents: 1,
        },
        EquityParams {
            seed: 1,
            mode: EquityMode::MonteCarlo { iterations: 1000 },
        },
    )
    .unwrap();
    assert!(e.equity >= 0.0 && e.equity <= 1.0);
    let seven: [Card; 7] = Deck::standard()[..7].try_into().unwrap();
    let _ = CactusKev.rank_seven(&seven);
    assert_eq!(d.len(), 52);
}
