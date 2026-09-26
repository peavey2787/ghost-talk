use super::realtime::*;

fn announce(dial_addresses: Vec<String>) -> TransportAnnounceV1 {
    TransportAnnounceV1 {
        peer_id: "peer".into(),
        dial_addresses,
        capabilities: vec![
            RealtimeCapability::AddressedDelivery,
            RealtimeCapability::Voice,
        ],
    }
}

#[test]
fn announcement_limits_fail_closed() {
    assert_eq!(announce(vec!["/a".into()]).validate(), Ok(()));
    assert_eq!(
        announce(vec![String::new()]).validate(),
        Err(RealtimeProtocolError::InvalidDialAddress)
    );
    assert_eq!(
        announce(vec!["x".repeat(MAX_P2P_DIAL_ADDRESS_BYTES + 1)]).validate(),
        Err(RealtimeProtocolError::InvalidDialAddress)
    );
    assert_eq!(
        announce(vec!["/a".into(); MAX_P2P_DIAL_ADDRESSES + 1]).validate(),
        Err(RealtimeProtocolError::TooManyDialAddresses)
    );
    let mut anonymous = announce(Vec::new());
    anonymous.peer_id = " ".into();
    assert_eq!(
        anonymous.validate(),
        Err(RealtimeProtocolError::EmptyPeerId)
    );
}

#[test]
fn transport_bodies_keep_their_wire_shape() {
    let body = RealtimeBodyV1::TransportAck(announce(Vec::new()));
    let json = serde_json::to_string(&body).unwrap();
    assert!(json.starts_with(r#"{"kind":"transport_ack","body":{"peer_id":"peer""#));
    let decoded: RealtimeBodyV1 = serde_json::from_str(&json).unwrap();
    assert!(decoded.is_ack());
    assert_eq!(decoded.announcement().peer_id, "peer");
    assert!(!RealtimeBodyV1::TransportAnnounce(announce(Vec::new())).is_ack());
}
