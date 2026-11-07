//! Shared wire model for the server, the CLI, and agent clients.
//!
//! One versioned schema is used by every audience: JSON output is the model,
//! and human output is a rendering of the same values.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::errors::{ErrorBody, ErrorCode};

/// Protocol version carried by every envelope.
pub const PROTOCOL_VERSION: u8 = 1;

/// Server identity reported by `hello` and `version`.
pub const SERVER_NAME: &str = "poker/0.1";

/// Table identifiers match `^[A-Za-z0-9_-]{1,64}$`.
pub type TableId = String;

/// Seats are 1-based and fixed at table creation.
pub type Seat = usize;

/// Monotonic, per-table hand number.
pub type HandNumber = u64;

// ---------------------------------------------------------------------------
// Envelopes
// ---------------------------------------------------------------------------

/// Client half of the `hello` handshake (the inner `hello` object).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub client: String,
    pub protocol: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Server half of the `hello` handshake (the inner `hello` object).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloAck {
    pub server: String,
    pub protocol: u8,
    pub socket: String,
}

/// A request line. `table` and `if_seq` are omitted when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub v: u8,
    pub req: String,
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default)]
    pub args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_seq: Option<u64>,
}

/// A response line. Field order is pinned: `v`, `req`, `ok`, `data`/`error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub req: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    /// Successful response carrying a JSON payload.
    pub fn ok(req: &str, data: impl Serialize) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            req: Some(req.to_owned()),
            ok: true,
            data: Some(serde_json::to_value(data).expect("response data must be serializable")),
            error: None,
        }
    }

    /// Failed response. `req` is `None` for handshake failures.
    pub fn err(req: Option<&str>, e: ErrorBody) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            req: req.map(str::to_owned),
            ok: false,
            data: None,
            error: Some(e),
        }
    }
}

/// Acknowledgement payload for mutating commands and stream bootstrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    pub schema: String,
    pub seq: u64,
}

impl Ack {
    /// Acknowledge the table's current `seq`.
    pub fn new(seq: u64) -> Self {
        Self {
            schema: "poker.ack/1".to_owned(),
            seq,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared wire model
// ---------------------------------------------------------------------------

/// Betting street.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

/// Per-seat status as projected from the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeatStatus {
    Active,
    Folded,
    AllIn,
    SittingOut,
}

/// Whether streets auto-advance or wait for an admin `step`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flow {
    Auto,
    Manual,
}

/// Table lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lifecycle {
    Waiting,
    Ready,
    InHand,
    BetweenHands,
    Finished,
}

/// Pot totals; `side` carries side-pot amounts in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pots {
    pub main: u64,
    pub side: Vec<u64>,
}

/// Table blinds and ante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blinds {
    pub sb: u64,
    pub bb: u64,
    pub ante: u64,
}

/// The seat on turn and its remaining clock, `null` when no clock is armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub seat: Seat,
    pub deadline_ms: Option<u64>,
}

/// Engine-computed menu for the calling seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalActions {
    pub fold: bool,
    pub check: bool,
    pub call: u64,
    pub raise: Option<RaiseRange>,
    pub all_in: u64,
}

/// Inclusive raise bounds, expressed as the total bet to raise to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaiseRange {
    pub min: u64,
    pub max: u64,
}

// ---------------------------------------------------------------------------
// Event model
// ---------------------------------------------------------------------------

/// A seat and its stack, used in `hand.begin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatStack {
    pub seat: Seat,
    pub stack: u64,
}

/// A blind or ante posting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blind {
    pub seat: Seat,
    pub amount: u64,
}

/// Hole cards exposed at showdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reveal {
    pub seat: Seat,
    pub hole: Vec<String>,
}

/// A seat receiving chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Winner {
    pub seat: Seat,
    pub amount: u64,
}

/// Why a turn clock fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeoutReason {
    Check,
    Fold,
    Timebank,
}

/// Why a table finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    End,
    SingleChipLeader,
}

/// Event payloads, tagged by wire `type`.
///
/// The event envelope (`schema`, `seq`) is added by the event log; this type is
/// the body that Tasks 2–16 produce and consume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventBody {
    #[serde(rename = "table.created")]
    TableCreated {
        seats: usize,
        sb: u64,
        bb: u64,
        ante: u64,
        button: Seat,
        stacks: BTreeMap<Seat, u64>,
        flow: Flow,
    },
    #[serde(rename = "seat.joined")]
    SeatJoined {
        seat: Seat,
        name: String,
        stack: u64,
    },
    #[serde(rename = "seat.left")]
    SeatLeft { seat: Seat },
    #[serde(rename = "seat.sat_out")]
    SeatSatOut { seat: Seat },
    #[serde(rename = "seat.sat_in")]
    SeatSatIn { seat: Seat },
    #[serde(rename = "hand.begin")]
    HandBegin {
        hand: HandNumber,
        button: Seat,
        seats: Vec<SeatStack>,
        stacks: BTreeMap<Seat, u64>,
    },
    #[serde(rename = "blinds.posted")]
    BlindsPosted {
        sb: Blind,
        bb: Blind,
        antes: Vec<Blind>,
    },
    /// A private deal (`seat` + `hole`) or a public board deal (`street` +
    /// `board`). Absent fields are omitted.
    #[serde(rename = "cards.dealt")]
    CardsDealt {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seat: Option<Seat>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hole: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        street: Option<Street>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        board: Option<Vec<String>>,
    },
    #[serde(rename = "action")]
    Action {
        seat: Seat,
        street: Street,
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<u64>,
    },
    #[serde(rename = "street.advanced")]
    StreetAdvanced {
        from: Street,
        to: Street,
        board: Vec<String>,
    },
    #[serde(rename = "timeout")]
    Timeout { seat: Seat, reason: TimeoutReason },
    #[serde(rename = "showdown")]
    Showdown {
        reveals: Vec<Reveal>,
        pots: Pots,
        winners: Vec<Winner>,
    },
    #[serde(rename = "hand.complete")]
    HandComplete {
        hand: HandNumber,
        winners: Vec<Winner>,
        stacks: BTreeMap<Seat, u64>,
    },
    #[serde(rename = "table.finished")]
    TableFinished { reason: FinishReason },
}
