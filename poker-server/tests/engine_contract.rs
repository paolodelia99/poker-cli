//! Contract tests for the server-owned engine seam.
//!
//! These tests pin the shape and fixture behavior the server codes against
//! until `poker-core`'s hand machine lands.

use poker_server::engine::{Action, Engine, EngineError, FixtureEngine, HandParams, Phase};
use poker_server::protocol::{EventBody, LegalActions, RaiseRange, SeatStatus, Street};

fn params(stacks: Vec<u64>) -> HandParams {
    HandParams {
        hand: 1,
        seats: stacks.len(),
        sb: 5,
        bb: 10,
        ante: 0,
        button: 1,
        stacks,
    }
}

#[test]
fn new_hand_posts_blinds_and_deals_privately() {
    let t = FixtureEngine.new_hand(&params(vec![1000, 1000, 1000]), 42);
    assert_eq!(t.hand.street, Street::Preflop);
    assert_eq!(t.hand.pots.main, 15);
    assert_eq!(t.hand.turn, Some(1)); // 3-handed: button/seat 1 is UTG preflop
    assert_eq!(
        t.events
            .iter()
            .filter(|e| matches!(e, EventBody::CardsDealt { hole: Some(_), .. }))
            .count(),
        3
    );
    assert!(t
        .events
        .iter()
        .any(|e| matches!(e, EventBody::BlindsPosted { .. })));
}

#[test]
fn legal_actions_facing_a_bet_matches_wire_shape() {
    let t = FixtureEngine.new_hand(&params(vec![1000, 1000]), 7);
    // Heads-up: button is SB, seat 2 is BB; seat 1 faces 5 to call.
    let menu = FixtureEngine.legal_actions(&t.hand, 1);
    assert_eq!(
        menu,
        LegalActions {
            fold: true,
            check: false,
            call: 5,
            raise: Some(RaiseRange { min: 20, max: 1000 }),
            all_in: 1000
        }
    );
}

#[test]
fn checking_the_round_closes_and_auto_advance_deals_flop() {
    let t = FixtureEngine.new_hand(&params(vec![1000, 1000]), 7);
    let t = FixtureEngine.apply(&t.hand, 1, Action::call()).unwrap(); // SB calls 5
    let t = FixtureEngine.apply(&t.hand, 2, Action::check()).unwrap();
    assert_eq!(t.hand.turn, None); // round closed, actor decides
    let t = FixtureEngine.auto_advance(&t.hand);
    assert_eq!(t.hand.street, Street::Flop);
    assert_eq!(t.hand.board.len(), 3);
}

#[test]
fn fold_to_one_player_completes_the_hand() {
    let t = FixtureEngine.new_hand(&params(vec![1000, 1000]), 11);
    let t = FixtureEngine.apply(&t.hand, 1, Action::fold()).unwrap();
    assert_eq!(t.phase, Phase::Complete);
    assert!(t
        .events
        .iter()
        .any(|e| matches!(e, EventBody::HandComplete { .. })));
    assert_eq!(t.hand.seats[1].stack, 1005); // BB wins the SB
}

#[test]
fn fold_out_of_turn_is_allowed_for_leave() {
    let t = FixtureEngine.new_hand(&params(vec![1000, 1000, 1000]), 3);
    let t = FixtureEngine.fold(&t.hand, 3).unwrap(); // seat 3 is not on turn
    assert_eq!(t.hand.seats[2].status, SeatStatus::Folded);
}

#[test]
fn same_seed_is_deterministic_and_different_seed_differs() {
    let a = FixtureEngine.new_hand(&params(vec![1000, 1000]), 99);
    let b = FixtureEngine.new_hand(&params(vec![1000, 1000]), 99);
    assert_eq!(a, b);
    let c = FixtureEngine.new_hand(&params(vec![1000, 1000]), 100);
    assert_ne!(a.hand.seats[0].hole, c.hand.seats[0].hole);
}

#[test]
fn apply_out_of_turn_and_after_complete_error() {
    let t = FixtureEngine.new_hand(&params(vec![1000, 1000]), 5);
    assert!(matches!(
        FixtureEngine.apply(&t.hand, 2, Action::check()),
        Err(EngineError::NotYourTurn { .. })
    ));
    let t = FixtureEngine.apply(&t.hand, 1, Action::fold()).unwrap();
    assert!(matches!(
        FixtureEngine.apply(&t.hand, 2, Action::check()),
        Err(EngineError::Complete)
    ));
}
