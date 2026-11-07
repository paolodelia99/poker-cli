//! The server-owned engine seam.
//!
//! The server never derives poker state itself: it asks the engine for the
//! legal-action menu, applies seat actions, and projects the resulting [`Hand`]
//! view into the shared wire model. [`fixture::FixtureEngine`] is the
//! deterministic stand-in used by server tests until `poker-core`'s hand
//! machine lands.

use std::fmt;

use poker_core::Card;
use serde_json::Value;

use crate::protocol::{ErrorBody, LegalActions, Pots, SeatStatus, Street};

pub mod fixture;

pub use crate::protocol::{HandNumber, Seat};
pub use fixture::FixtureEngine;

/// Hand lifecycle as seen by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Betting,
    Complete,
}

/// A hand's full engine-side view.
///
/// The server's read surface is the public fields; `bb` and `remaining` are
/// fixture bookkeeping that keep the fixture stateless (all behavior derives
/// from `&Hand`).
#[derive(Debug, Clone, PartialEq)]
pub struct Hand {
    pub hand: HandNumber,
    pub street: Street,
    pub board: Vec<Card>,
    pub seats: Vec<SeatHand>,
    pub pots: Pots,
    pub button: Seat,
    pub current_bet: u64,
    pub min_raise_to: u64,
    pub turn: Option<Seat>,
    pub phase: Phase,
    /// Big blind, remembered for postflop minimum bets.
    bb: u64,
    /// Undealt cards in shuffle order, so streets can advance deterministically.
    remaining: Vec<Card>,
}

/// Per-seat engine view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatHand {
    pub seat: Seat,
    pub stack: u64,
    pub bet: u64,
    pub status: SeatStatus,
    pub hole: Option<[Card; 2]>,
    pub last_action: Option<String>,
    pub acted: bool,
}

/// Inputs for a new hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandParams {
    pub hand: HandNumber,
    pub seats: usize,
    pub sb: u64,
    pub bb: u64,
    pub ante: u64,
    pub button: Seat,
    pub stacks: Vec<u64>,
}

/// The action chosen by a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Fold,
    Check,
    Call,
    Raise { to: Option<u64>, by: Option<u64> },
    AllIn,
}

/// A seat action, wire-agnostic once parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub kind: ActionKind,
}

impl Action {
    pub fn fold() -> Self {
        Self {
            kind: ActionKind::Fold,
        }
    }

    pub fn check() -> Self {
        Self {
            kind: ActionKind::Check,
        }
    }

    pub fn call() -> Self {
        Self {
            kind: ActionKind::Call,
        }
    }

    pub fn raise_to(to: u64) -> Self {
        Self {
            kind: ActionKind::Raise {
                to: Some(to),
                by: None,
            },
        }
    }

    pub fn raise_by(by: u64) -> Self {
        Self {
            kind: ActionKind::Raise {
                to: None,
                by: Some(by),
            },
        }
    }

    pub fn all_in() -> Self {
        Self {
            kind: ActionKind::AllIn,
        }
    }

    /// Parse the `choose` action object: `{"action":"fold"}`, `…:"check"`,
    /// `…:"call"`, `…:"raise","to":60` / `…"by":60`, or `…:"all-in"`.
    pub fn from_args(args: &Value) -> Result<Action, ErrorBody> {
        let obj = args
            .as_object()
            .ok_or_else(|| ErrorBody::usage("action arguments must be an object"))?;
        let name = obj
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| ErrorBody::usage("action object requires a string 'action'"))?;

        match name {
            "fold" => Ok(Self::fold()),
            "check" => Ok(Self::check()),
            "call" => Ok(Self::call()),
            "all-in" | "all_in" => Ok(Self::all_in()),
            "raise" => {
                let to = obj.get("to").and_then(Value::as_u64);
                let by = obj.get("by").and_then(Value::as_u64);
                match (to, by) {
                    (Some(to), None) => Ok(Self::raise_to(to)),
                    (None, Some(by)) => Ok(Self::raise_by(by)),
                    (Some(_), Some(_)) => Err(ErrorBody::usage(
                        "raise takes either 'to' or 'by', not both",
                    )),
                    (None, None) => Err(ErrorBody::usage("raise requires 'to' or 'by'")),
                }
            }
            other => Err(ErrorBody::usage(format!("unknown action: {other}"))),
        }
    }
}

/// A state transition plus the events it produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub hand: Hand,
    pub events: Vec<crate::protocol::EventBody>,
    pub phase: Phase,
}

/// Engine failures the server maps onto wire errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    NotYourTurn { turn: Option<Seat> },
    IllegalAction(String),
    Complete,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotYourTurn { turn } => match turn {
                Some(seat) => write!(f, "not your turn; seat {seat} is on turn"),
                None => write!(f, "not your turn; no seat is on turn"),
            },
            Self::IllegalAction(message) => write!(f, "illegal action: {message}"),
            Self::Complete => write!(f, "hand is complete"),
        }
    }
}

impl std::error::Error for EngineError {}

/// The poker authority seam consumed by the server.
pub trait Engine: Send + Sync + 'static {
    fn new_hand(&self, params: &HandParams, seed: u64) -> Transition;
    fn legal_actions(&self, hand: &Hand, seat: Seat) -> LegalActions;
    fn apply(&self, hand: &Hand, seat: Seat, action: Action) -> Result<Transition, EngineError>;
    fn fold(&self, hand: &Hand, seat: Seat) -> Result<Transition, EngineError>;
    fn auto_advance(&self, hand: &Hand) -> Transition;
    fn step(&self, hand: &Hand) -> Transition;
}
