//! The canonical, per-table event log.
//!
//! Every accepted intent produces exactly one set of engine events; the actor
//! stamps each with the next `seq`, appends it here, and broadcasts it. The log
//! holds the full truth; [`crate::views::event_for`] is the only redactor that
//! turns an event into bytes for a connection.

use serde::{Deserialize, Serialize};

use crate::protocol::EventBody;

/// Schema tag carried by every event envelope.
pub const EVENT_SCHEMA: &str = "poker.event/1";

/// A sequenced event: the envelope (`schema`, `seq`) plus a flattened body.
///
/// Serialized flat so the wire shape is
/// `{"schema":..,"seq":..,"type":..,<payload fields>}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub schema: String,
    pub seq: u64,
    #[serde(flatten)]
    pub body: EventBody,
}

/// Append-only event log owned by a table actor.
///
/// `seq` starts at 1 and only ever increases. `append` is the single place that
/// assigns sequence numbers, so no caller can produce a gap or a rewind.
#[derive(Debug, Clone, Default)]
pub struct EventLog {
    events: Vec<Event>,
}

impl EventLog {
    /// An empty log; the next appended event gets `seq == 1`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a body, stamping it with the next sequence number.
    pub fn append(&mut self, body: EventBody) -> Event {
        let seq = self.last_seq() + 1;
        let event = Event {
            schema: EVENT_SCHEMA.to_owned(),
            seq,
            body,
        };
        self.events.push(event.clone());
        event
    }

    /// Every event with `seq` strictly greater than `since`.
    pub fn since(&self, since: u64) -> Vec<Event> {
        self.events
            .iter()
            .filter(|event| event.seq > since)
            .cloned()
            .collect()
    }

    /// The highest assigned sequence number, `0` when the log is empty.
    pub fn last_seq(&self) -> u64 {
        self.events.last().map_or(0, |event| event.seq)
    }

    /// The full log, oldest first.
    pub fn all(&self) -> &[Event] {
        &self.events
    }
}
