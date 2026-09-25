use super::realtime::*;

#[test]
fn gtr1_round_trip_preserves_exact_carrier_bytes() {
    let envelope = Gtr1Envelope::new([1; 32], [2; 16], [3; 16], vec![4, 5, 6]).unwrap();
    let encoded = envelope.encode().unwrap();
    assert_eq!(&encoded[..4], b"GTR1");
    assert_eq!(Gtr1Envelope::decode(&encoded).unwrap(), envelope);
}

#[test]
fn gtr1_rejects_truncated_and_empty_ciphertext() {
    assert_eq!(Gtr1Envelope::decode(b"GTR1"), Err(RealtimeProtocolError::Truncated));
    assert_eq!(
        Gtr1Envelope::new([0; 32], [0; 16], [0; 16], Vec::new()),
        Err(RealtimeProtocolError::InvalidCiphertextLength)
    );
}

#[test]
fn session_topic_is_deterministic_and_not_plain_sid() {
    let sid = [7; 16];
    let topic = session_topic(&sid);
    assert_eq!(topic, session_topic(&sid));
    assert!(topic.starts_with("ghost-talk/realtime/v1/"));
    assert!(!topic.contains(&hex::encode(sid)));
}

#[test]
fn announcement_and_voice_limits_fail_closed() {
    let announce = TransportAnnounceV1 {
        peer_id: "peer".into(),
        dial_addresses: vec![String::new()],
        capabilities: Vec::new(),
    };
    assert_eq!(announce.validate(), Err(RealtimeProtocolError::InvalidDialAddress));
    let voice = VoiceBatchV1 { stream_id: "s".into(), frames: Vec::new() };
    assert_eq!(voice.validate(), Err(RealtimeProtocolError::InvalidVoiceBatch));
}
