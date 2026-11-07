//! Persisted hand documents (`poker.hand/1`).
//!
//! This task pins only the wire types. Task 10 adds `build_document` and the
//! single history writer; the document is always a public projection produced
//! through [`crate::views::event_for`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::events::Event;
use crate::protocol::{HandNumber, Seat, TableId};

/// Schema tag of a persisted hand document.
pub const HAND_SCHEMA: &str = "poker.hand/1";

/// A completed hand, self-describing and replayable without earlier hands.
///
/// `events` must already be the public projection: no undisclosed hole cards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandDocument {
    pub schema: String,
    pub table: TableId,
    pub hand: HandNumber,
    pub params: DocumentParams,
    pub events: Vec<Event>,
}

/// Table and hand parameters embedded in a hand document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentParams {
    pub seats: usize,
    pub sb: u64,
    pub bb: u64,
    pub ante: u64,
    pub button: Seat,
    pub stacks: BTreeMap<Seat, u64>,
}
