//! Deterministic, in-memory engine used by server tests.
//!
//! The fixture is intentionally small: it implements enough No-Limit Hold'em to
//! exercise the server's projection, clock, and event-log paths. It is
//! **stateless** — every operation derives from the supplied [`Hand`], which
//! carries the undealt deck remainder and big-blind size as fixture
//! bookkeeping.

use std::collections::BTreeMap;

use poker_core::{CactusKev, Card, Deck, HandEvaluator, HandRank};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;

use super::{
    Action, ActionKind, Engine, EngineError, Hand, HandParams, Phase, Seat, SeatHand, Transition,
};
use crate::protocol::{
    Blind, EventBody, LegalActions, Pots, RaiseRange, Reveal, SeatStack, SeatStatus, Street, Winner,
};

/// Stateless fixture engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct FixtureEngine;

impl Engine for FixtureEngine {
    fn new_hand(&self, params: &HandParams, seed: u64) -> Transition {
        let count = params.seats;
        let starting: Vec<u64> = (1..=count)
            .map(|seat| params.stacks.get(seat - 1).copied().unwrap_or(0))
            .collect();

        let mut hand = Hand {
            hand: params.hand,
            street: Street::Preflop,
            board: Vec::new(),
            seats: starting
                .iter()
                .enumerate()
                .map(|(index, &stack)| SeatHand {
                    seat: index + 1,
                    stack,
                    bet: 0,
                    status: if stack > 0 {
                        SeatStatus::Active
                    } else {
                        SeatStatus::SittingOut
                    },
                    hole: None,
                    last_action: None,
                    acted: false,
                })
                .collect(),
            pots: Pots {
                main: 0,
                side: Vec::new(),
            },
            button: params.button,
            current_bet: 0,
            min_raise_to: params.bb.saturating_mul(2),
            turn: None,
            phase: Phase::Betting,
            bb: params.bb,
            remaining: Vec::new(),
        };

        let is_active = |seat: Seat| starting.get(seat - 1).copied().unwrap_or(0) > 0;
        let active: Vec<Seat> = clockwise_after(params.button, count)
            .filter(|&seat| is_active(seat))
            .collect();

        // Shuffle once, then deal two cards per active seat, one at a time,
        // starting left of the button.
        let mut deck = Deck::standard();
        let mut rng = ChaCha12Rng::seed_from_u64(seed);
        deck.shuffle(&mut rng);

        let mut dealt: Vec<Option<[Card; 2]>> = vec![None; count];
        let mut next_card = 0_usize;
        for round in 0..2 {
            for &seat in &active {
                let card = deck[next_card];
                next_card += 1;
                let pair = dealt[seat - 1].get_or_insert([card; 2]);
                pair[round] = card;
            }
        }
        for (index, hole) in dealt.into_iter().enumerate() {
            hand.seats[index].hole = hole;
        }
        hand.remaining = deck[next_card..].to_vec();

        // Antes are dead money: they move straight into the pot and are not
        // counted toward the current street bet.
        let mut antes = Vec::new();
        if params.ante > 0 {
            for seat in 1..=count {
                if !is_active(seat) {
                    continue;
                }
                let paid = post_chips(&mut hand, seat, params.ante, false);
                if paid > 0 {
                    antes.push(Blind { seat, amount: paid });
                }
            }
        }

        // Heads-up the button posts the small blind; otherwise the small blind
        // is the next active seat after the button.
        let active_count = active.len();
        let sb_seat = if active_count == 2 && is_active(params.button) {
            params.button
        } else {
            clockwise_after(params.button, count)
                .find(|&seat| is_active(seat))
                .unwrap_or(params.button)
        };
        let bb_seat = clockwise_after(sb_seat, count)
            .find(|&seat| is_active(seat) && seat != sb_seat)
            .unwrap_or(sb_seat);

        let sb_paid = post_chips(&mut hand, sb_seat, params.sb, true);
        let bb_paid = post_chips(&mut hand, bb_seat, params.bb, true);

        hand.current_bet = params.bb;
        hand.min_raise_to = params.bb.saturating_mul(2);
        hand.turn = next_to_act(&hand, bb_seat);

        let mut events = Vec::new();
        let stacks_map = starting
            .iter()
            .enumerate()
            .map(|(index, &stack)| (index + 1, stack))
            .collect::<BTreeMap<Seat, u64>>();
        let seat_stacks = stacks_map
            .iter()
            .map(|(&seat, &stack)| SeatStack { seat, stack })
            .collect();
        events.push(EventBody::HandBegin {
            hand: params.hand,
            button: params.button,
            seats: seat_stacks,
            stacks: stacks_map,
        });
        events.push(EventBody::BlindsPosted {
            sb: Blind {
                seat: sb_seat,
                amount: sb_paid,
            },
            bb: Blind {
                seat: bb_seat,
                amount: bb_paid,
            },
            antes,
        });
        for &seat in &active {
            if let Some(hole) = hand.seats[seat - 1].hole {
                events.push(EventBody::CardsDealt {
                    seat: Some(seat),
                    hole: Some(vec![hole[0].display(), hole[1].display()]),
                    street: None,
                    board: None,
                });
            }
        }

        let phase = hand.phase;
        Transition {
            hand,
            events,
            phase,
        }
    }

    fn legal_actions(&self, hand: &Hand, seat: Seat) -> LegalActions {
        let Some(player) = hand.seats.iter().find(|player| player.seat == seat) else {
            return LegalActions {
                fold: true,
                check: false,
                call: 0,
                raise: None,
                all_in: 0,
            };
        };

        let call = hand
            .current_bet
            .saturating_sub(player.bet)
            .min(player.stack);
        let max_total = player.stack.saturating_add(player.bet);
        let raise = if player.stack > call && max_total >= hand.min_raise_to {
            Some(RaiseRange {
                min: hand.min_raise_to,
                max: max_total,
            })
        } else {
            None
        };

        LegalActions {
            fold: true,
            check: player.bet == hand.current_bet,
            call,
            raise,
            all_in: max_total,
        }
    }

    fn apply(&self, hand: &Hand, seat: Seat, action: Action) -> Result<Transition, EngineError> {
        if hand.phase == Phase::Complete {
            return Err(EngineError::Complete);
        }
        if hand.turn != Some(seat) {
            return Err(EngineError::NotYourTurn { turn: hand.turn });
        }

        let position = position_of(hand, seat)?;
        if !can_act(&hand.seats[position]) {
            return Err(EngineError::IllegalAction(format!(
                "seat {seat} cannot act"
            )));
        }

        let mut next = hand.clone();
        let current_bet = next.current_bet;
        let min_raise_to = next.min_raise_to;
        let bet = next.seats[position].bet;
        let stack = next.seats[position].stack;

        let (action_string, amount) = match action.kind {
            ActionKind::Fold => {
                next.seats[position].status = SeatStatus::Folded;
                ("fold".to_owned(), None)
            }
            ActionKind::Check => {
                if bet != current_bet {
                    return Err(EngineError::IllegalAction(
                        "cannot check facing a bet".to_owned(),
                    ));
                }
                ("check".to_owned(), None)
            }
            ActionKind::Call => {
                let call = current_bet.saturating_sub(bet).min(stack);
                next.seats[position].stack -= call;
                next.seats[position].bet += call;
                next.pots.main = next.pots.main.saturating_add(call);
                if next.seats[position].stack == 0 {
                    next.seats[position].status = SeatStatus::AllIn;
                }
                (format!("call {call}"), Some(call))
            }
            ActionKind::Raise { to, by } => {
                let target = match (to, by) {
                    (Some(_), Some(_)) => {
                        return Err(EngineError::IllegalAction(
                            "raise takes either 'to' or 'by', not both".to_owned(),
                        ))
                    }
                    (Some(to), None) => to,
                    (None, Some(by)) => bet.saturating_add(by),
                    (None, None) => {
                        return Err(EngineError::IllegalAction(
                            "raise requires 'to' or 'by'".to_owned(),
                        ))
                    }
                };
                if target <= current_bet {
                    return Err(EngineError::IllegalAction(format!(
                        "raise to {target} must exceed current bet {current_bet}"
                    )));
                }
                if target < min_raise_to {
                    return Err(EngineError::IllegalAction(format!(
                        "raise to {target} is below the minimum {min_raise_to}"
                    )));
                }
                let max_total = stack.saturating_add(bet);
                if target > max_total {
                    return Err(EngineError::IllegalAction(format!(
                        "raise to {target} exceeds stack {max_total}"
                    )));
                }
                let additional = target - bet;
                next.seats[position].stack -= additional;
                next.seats[position].bet = target;
                next.pots.main = next.pots.main.saturating_add(additional);
                if next.seats[position].stack == 0 {
                    next.seats[position].status = SeatStatus::AllIn;
                }
                next.min_raise_to = target.saturating_add(target - current_bet);
                next.current_bet = target;
                (format!("raise to {target}"), Some(target))
            }
            ActionKind::AllIn => {
                if stack == 0 {
                    return Err(EngineError::IllegalAction(
                        "no chips to go all-in with".to_owned(),
                    ));
                }
                let total = stack.saturating_add(bet);
                next.pots.main = next.pots.main.saturating_add(stack);
                next.seats[position].bet = total;
                next.seats[position].stack = 0;
                next.seats[position].status = SeatStatus::AllIn;
                if total > current_bet {
                    if total >= min_raise_to {
                        next.min_raise_to = total.saturating_add(total - current_bet);
                    }
                    next.current_bet = total;
                }
                (format!("all-in {total}"), Some(total))
            }
        };

        next.seats[position].acted = true;
        next.seats[position].last_action = Some(action_string.clone());
        let events = vec![EventBody::Action {
            seat,
            street: next.street,
            action: action_string,
            amount,
        }];
        Ok(finish_after_action(next, seat, events))
    }

    fn fold(&self, hand: &Hand, seat: Seat) -> Result<Transition, EngineError> {
        if hand.phase == Phase::Complete {
            return Err(EngineError::Complete);
        }
        let position = position_of(hand, seat)?;
        if hand.seats[position].status == SeatStatus::Folded {
            return Err(EngineError::IllegalAction(format!(
                "seat {seat} has already folded"
            )));
        }

        let mut next = hand.clone();
        next.seats[position].status = SeatStatus::Folded;
        next.seats[position].acted = true;
        next.seats[position].last_action = Some("fold".to_owned());
        let mut events = vec![EventBody::Action {
            seat,
            street: next.street,
            action: "fold".to_owned(),
            amount: None,
        }];

        if in_hand_count(&next) == 1 {
            events.extend(complete_by_fold(&mut next));
        } else if next.turn == Some(seat) {
            next.turn = next_to_act(&next, seat);
        }

        let phase = next.phase;
        Ok(Transition {
            hand: next,
            events,
            phase,
        })
    }

    fn auto_advance(&self, hand: &Hand) -> Transition {
        if hand.phase == Phase::Complete {
            return Transition {
                hand: hand.clone(),
                events: Vec::new(),
                phase: hand.phase,
            };
        }
        if hand.turn.is_some() {
            // The trait has no error channel here. The server only calls
            // `auto_advance` once a betting round has closed, so a still-open
            // round is a no-op rather than an error.
            return Transition {
                hand: hand.clone(),
                events: Vec::new(),
                phase: hand.phase,
            };
        }

        let mut next = hand.clone();
        let from = next.street;
        let mut events = Vec::new();
        let to = match from {
            Street::Preflop => {
                deal_board(&mut next, 3);
                Street::Flop
            }
            Street::Flop => {
                deal_board(&mut next, 1);
                Street::Turn
            }
            Street::Turn => {
                deal_board(&mut next, 1);
                Street::River
            }
            Street::River => {
                events.extend(showdown(&mut next));
                let phase = next.phase;
                return Transition {
                    hand: next,
                    events,
                    phase,
                };
            }
        };

        let board: Vec<String> = next.board.iter().map(Card::display).collect();
        events.push(EventBody::CardsDealt {
            seat: None,
            hole: None,
            street: Some(to),
            board: Some(board.clone()),
        });
        events.push(EventBody::StreetAdvanced { from, to, board });

        next.street = to;
        for player in &mut next.seats {
            player.bet = 0;
            player.acted = false;
        }
        next.current_bet = 0;
        next.min_raise_to = next.bb;
        next.turn = next_to_act(&next, next.button);
        next.phase = Phase::Betting;

        let phase = next.phase;
        Transition {
            hand: next,
            events,
            phase,
        }
    }

    fn step(&self, hand: &Hand) -> Transition {
        self.auto_advance(hand)
    }
}

/// Seats clockwise after `from`, excluding `from`, wrapping once.
fn clockwise_after(from: Seat, count: usize) -> impl Iterator<Item = Seat> {
    (1..=count).map(move |offset| ((from - 1 + offset) % count) + 1)
}

/// Whether a seat may still make a betting decision.
fn can_act(player: &SeatHand) -> bool {
    player.status == SeatStatus::Active && player.stack > 0
}

/// Whether a seat is still contesting the pot (active or all-in).
fn in_hand(player: &SeatHand) -> bool {
    matches!(player.status, SeatStatus::Active | SeatStatus::AllIn)
}

fn in_hand_count(hand: &Hand) -> usize {
    hand.seats.iter().filter(|player| in_hand(player)).count()
}

fn position_of(hand: &Hand, seat: Seat) -> Result<usize, EngineError> {
    hand.seats
        .iter()
        .position(|player| player.seat == seat)
        .ok_or_else(|| EngineError::IllegalAction(format!("no such seat: {seat}")))
}

/// Move chips into the pot, optionally counting them toward the street bet.
fn post_chips(hand: &mut Hand, seat: Seat, amount: u64, to_bet: bool) -> u64 {
    let Some(player) = hand.seats.get_mut(seat - 1) else {
        return 0;
    };
    let paid = amount.min(player.stack);
    player.stack -= paid;
    if to_bet {
        player.bet = player.bet.saturating_add(paid);
    }
    if player.stack == 0 {
        player.status = SeatStatus::AllIn;
    }
    hand.pots.main = hand.pots.main.saturating_add(paid);
    paid
}

/// Next seat clockwise that still owes a decision this round.
fn next_to_act(hand: &Hand, from: Seat) -> Option<Seat> {
    clockwise_after(from, hand.seats.len()).find(|&seat| {
        let player = &hand.seats[seat - 1];
        can_act(player) && (!player.acted || player.bet != hand.current_bet)
    })
}

fn finish_after_action(mut hand: Hand, actor: Seat, mut events: Vec<EventBody>) -> Transition {
    if in_hand_count(&hand) == 1 {
        events.extend(complete_by_fold(&mut hand));
    } else {
        hand.turn = next_to_act(&hand, actor);
    }

    let phase = hand.phase;
    Transition {
        hand,
        events,
        phase,
    }
}

/// Award the whole pot to the last seat still in the hand.
fn complete_by_fold(hand: &mut Hand) -> Vec<EventBody> {
    let winner = hand
        .seats
        .iter()
        .find(|player| in_hand(player))
        .map(|player| player.seat);
    let amount = hand
        .pots
        .main
        .saturating_add(hand.pots.side.iter().sum::<u64>());

    let mut winners = Vec::new();
    if let Some(seat) = winner {
        if let Some(player) = hand.seats.get_mut(seat - 1) {
            player.stack = player.stack.saturating_add(amount);
        }
        winners.push(Winner { seat, amount });
    }

    hand.pots.main = 0;
    hand.pots.side.clear();
    hand.phase = Phase::Complete;
    hand.turn = None;

    vec![EventBody::HandComplete {
        hand: hand.hand,
        winners,
        stacks: stacks_map(hand),
    }]
}

/// Deal the next street's board cards from the retained deck remainder.
fn deal_board(hand: &mut Hand, count: usize) {
    let take = count.min(hand.remaining.len());
    let cards: Vec<Card> = hand.remaining.drain(..take).collect();
    hand.board.extend(cards);
}

fn showdown(hand: &mut Hand) -> Vec<EventBody> {
    let contenders: Vec<Seat> = hand
        .seats
        .iter()
        .filter(|player| in_hand(player))
        .map(|player| player.seat)
        .collect();

    let board = hand.board.clone();
    let mut ranks: BTreeMap<Seat, HandRank> = BTreeMap::new();
    if board.len() >= 5 {
        let board = [board[0], board[1], board[2], board[3], board[4]];
        for &seat in &contenders {
            if let Some(hole) = hand.seats[seat - 1].hole {
                let mut cards = [board[0]; 7];
                cards[..5].copy_from_slice(&board);
                cards[5..].copy_from_slice(&hole);
                ranks.insert(seat, CactusKev.rank_seven(&cards));
            }
        }
    }

    let best = ranks.values().copied().max();
    let mut best_seats: Vec<Seat> = ranks
        .iter()
        .filter(|(_, rank)| Some(**rank) == best)
        .map(|(&seat, _)| seat)
        .collect();
    if best_seats.is_empty() {
        best_seats = contenders.clone();
    }

    let ordered = order_from_button(hand, &best_seats);
    let pot = hand
        .pots
        .main
        .saturating_add(hand.pots.side.iter().sum::<u64>());
    let winner_count = ordered.len() as u64;
    let share = pot.checked_div(winner_count).unwrap_or(0);
    let remainder = pot.checked_rem(winner_count).unwrap_or(0);

    let mut payouts = Vec::new();
    for (index, seat) in ordered.into_iter().enumerate() {
        let odd_chip = if (index as u64) < remainder { 1 } else { 0 };
        let amount = share + odd_chip;
        hand.seats[seat - 1].stack = hand.seats[seat - 1].stack.saturating_add(amount);
        payouts.push(Winner { seat, amount });
    }

    let reveals: Vec<Reveal> = order_from_button(hand, &contenders)
        .into_iter()
        .filter_map(|seat| {
            hand.seats[seat - 1].hole.map(|hole| Reveal {
                seat,
                hole: vec![hole[0].display(), hole[1].display()],
            })
        })
        .collect();

    let pots = hand.pots.clone();
    hand.pots.main = 0;
    hand.pots.side.clear();
    hand.phase = Phase::Complete;
    hand.turn = None;

    let winners = payouts.clone();
    vec![
        EventBody::Showdown {
            reveals,
            pots,
            winners: payouts,
        },
        EventBody::HandComplete {
            hand: hand.hand,
            winners,
            stacks: stacks_map(hand),
        },
    ]
}

/// Order a subset of seats clockwise from the button.
fn order_from_button(hand: &Hand, seats: &[Seat]) -> Vec<Seat> {
    clockwise_after(hand.button, hand.seats.len())
        .filter(|seat| seats.contains(seat))
        .collect()
}

fn stacks_map(hand: &Hand) -> BTreeMap<Seat, u64> {
    hand.seats
        .iter()
        .map(|player| (player.seat, player.stack))
        .collect()
}
