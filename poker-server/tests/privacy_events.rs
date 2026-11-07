use poker_server::events::{Event, EventLog};
use poker_server::protocol::{EventBody, Pots, Reveal, Winner};
use poker_server::views::{self, Observer};

fn deal_event() -> Event {
    let mut log = EventLog::new();
    log.append(EventBody::CardsDealt {
        seat: Some(2),
        hole: Some(vec!["Qs".into(), "Qh".into()]),
        street: None,
        board: None,
    })
}

#[test]
fn seq_starts_at_one_and_is_monotonic() {
    let mut log = EventLog::new();
    assert_eq!(log.append(EventBody::SeatLeft { seat: 1 }).seq, 1);
    assert_eq!(log.append(EventBody::SeatLeft { seat: 2 }).seq, 2);
    assert_eq!(log.last_seq(), 2);
    assert_eq!(
        log.since(1).iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![2]
    );
}

#[test]
fn owner_sees_hole_cards() {
    let e = deal_event();
    let json = serde_json::to_value(views::event_for(&Observer::Player(2), &e)).unwrap();
    assert_eq!(json["hole"], serde_json::json!(["Qs", "Qh"]));
}

#[test]
fn non_owner_spectator_and_admin_without_reveal_lose_the_hole_field() {
    for observer in [
        Observer::Anonymous,
        Observer::Spectator,
        Observer::Player(1),
        Observer::Admin { reveal: false },
    ] {
        let e = deal_event();
        let bytes = serde_json::to_string(&views::event_for(&observer, &e)).unwrap();
        assert!(!bytes.contains("Qs"), "{observer:?} leaked: {bytes}");
        assert!(bytes.contains(r#""seat":2"#));
        assert!(!bytes.contains("hole"));
    }
}

#[test]
fn showdown_reveals_stay_public() {
    let mut log = EventLog::new();
    let e = log.append(EventBody::Showdown {
        reveals: vec![Reveal {
            seat: 1,
            hole: vec!["As".into(), "Kd".into()],
        }],
        pots: Pots {
            main: 100,
            side: vec![],
        },
        winners: vec![Winner {
            seat: 1,
            amount: 100,
        }],
    });
    let json = serde_json::to_value(views::event_for(&Observer::Spectator, &e)).unwrap();
    assert_eq!(json["reveals"][0]["hole"], serde_json::json!(["As", "Kd"]));
}

#[test]
fn wire_event_is_flat_and_pinned() {
    let e = deal_event();
    assert_eq!(
        serde_json::to_string(&views::event_for(&Observer::Player(2), &e)).unwrap(),
        r#"{"schema":"poker.event/1","seq":1,"type":"cards.dealt","seat":2,"hole":["Qs","Qh"]}"#
    );
}
