//! Perspective-scoped projections: the only place server state becomes bytes.
//!
//! [`event_for`] is the single event redaction choke point. No other module may
//! serialize a hole card from an event.

use crate::events::Event;
use crate::protocol::{EventBody, Seat};

/// Who is reading a table. Drives every redaction decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observer {
    /// No token; sees only public data.
    Anonymous,
    /// A token for the current hand (or a bystander).
    Spectator,
    /// The seat may see its own hole cards.
    Player(Seat),
    /// Admin. `reveal` applies to `snapshot --reveal` only, never to events.
    Admin { reveal: bool },
}

/// Project an event for one observer.
///
/// Private `cards.dealt` hole cards are visible only to the owning
/// [`Observer::Player`]. Spectators, anonymous readers, and other players lose
/// the `hole` field entirely. Admin is **not** an exception on the event stream;
/// only the admin `snapshot --reveal` path discloses cards. `seat`, `street`,
/// and `board` are always preserved, and showdown reveals stay public.
pub fn event_for(observer: &Observer, event: &Event) -> Event {
    let mut projected = event.clone();
    if let EventBody::CardsDealt { seat, hole, .. } = &mut projected.body {
        let owns = matches!(observer, Observer::Player(player) if *seat == Some(*player));
        if !owns {
            *hole = None;
        }
    }
    projected
}
