use poker_server::protocol::*;

#[test]
fn ok_response_has_no_error_field() {
    let r = Response::ok("r-1", Ack::new(7));
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"v":1,"req":"r-1","ok":true,"data":{"schema":"poker.ack/1","seq":7}}"#
    );
}

#[test]
fn error_envelope_matches_cli_contract() {
    let mut e = ErrorBody::new(ErrorCode::NotYourTurn, "seat 2 is not on turn");
    e.turn = Some(Turn {
        seat: 1,
        deadline_ms: Some(11840),
    });
    let value = serde_json::to_value(Response::err(Some("r-42"), e)).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "not_your_turn");
    assert_eq!(value["error"]["turn"]["deadline_ms"], 11840);
    assert!(value["error"].get("legal_actions").is_none());
}

#[test]
fn handshake_error_omits_req() {
    let value = serde_json::to_value(Response::err(
        None,
        ErrorBody::new(ErrorCode::ProtocolVersion, "bad"),
    ))
    .unwrap();
    assert!(value.get("req").is_none());
}

#[test]
fn exit_codes_match_server_spec() {
    let cases = [
        (ErrorCode::Internal, 1),
        (ErrorCode::Usage, 2),
        (ErrorCode::ProtocolVersion, 5),
        (ErrorCode::Auth, 6),
        (ErrorCode::NotFound, 7),
        (ErrorCode::NotYourTurn, 10),
        (ErrorCode::IllegalAction, 11),
        (ErrorCode::Stale, 12),
        (ErrorCode::TableFinished, 13),
        (ErrorCode::WaitTimeout, 14),
    ];
    for (code, exit) in cases {
        assert_eq!(code.exit_code(), exit, "{code:?}");
    }
}

#[test]
fn hello_round_trips_and_token_is_optional() {
    let h: Hello = serde_json::from_str(r#"{"client":"poker-cli/0.1","protocol":1}"#).unwrap();
    assert_eq!(h.token, None);
    let h: Hello = serde_json::from_str(r#"{"client":"c","protocol":1,"token":"abc"}"#).unwrap();
    assert_eq!(h.token.as_deref(), Some("abc"));
}

#[test]
fn legal_actions_match_the_cli_example() {
    let menu = LegalActions {
        fold: true,
        check: false,
        call: 20,
        raise: Some(RaiseRange { min: 40, max: 940 }),
        all_in: 940,
    };
    assert_eq!(
        serde_json::to_string(&menu).unwrap(),
        r#"{"fold":true,"check":false,"call":20,"raise":{"min":40,"max":940},"all_in":940}"#
    );
}
