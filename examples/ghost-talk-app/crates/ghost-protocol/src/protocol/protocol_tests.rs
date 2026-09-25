use super::{
    call_signal::GhostCallSignal,
    events::{
        fragment, CarrierFrame, GhostCallInviteContext, GhostContactDescriptor,
        GhostContactRequest, BASE64, GHOST_KKTP_VERSION, GHST_DATA_MAX, GTCR_MAGIC,
        KKTP_ANCHOR_PREFIX, MAX_FRAGMENTS, MAX_GHOST_TX_PAYLOAD, MAX_KSPT_V1_PAYLOAD_BYTES,
    },
    kktp_codec::{kktp_anchor_type, kktp_mailbox_id},
    kktp_types::{
        GhostContactAccept, KktpDirection, KktpInnerMessage, KktpMailboxMessage, KktpSessionEnd,
    },
};
#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use ghost_core::Id128;

    #[test]
    fn fragments_roundtrip_across_kspt_v1_boundary() {
        let data = vec![0x5a; GHST_DATA_MAX + 17];
        let packet = Id128([7; 16]);
        let frames = fragment(packet, &data).unwrap();
        assert_eq!(frames.len(), 2);
        assert!(frames
            .iter()
            .all(|raw| raw.len() <= MAX_KSPT_V1_PAYLOAD_BYTES));
        let mut rebuilt = Vec::new();
        for raw in frames {
            rebuilt.extend(CarrierFrame::decode(&raw).unwrap().payload);
        }
        assert_eq!(rebuilt, data);
    }

    #[test]
    fn eighty_kib_logical_carrier_only_fragments_at_real_kspt_v1_boundary() {
        let data = vec![0xa5; MAX_GHOST_TX_PAYLOAD];
        let packet = Id128([9; 16]);
        let frames = fragment(packet, &data).unwrap();
        assert!(frames.len() > 1);
        assert!(frames.len() <= MAX_FRAGMENTS);
        assert!(frames
            .iter()
            .all(|raw| raw.len() <= MAX_KSPT_V1_PAYLOAD_BYTES));
        let rebuilt = frames
            .iter()
            .flat_map(|raw| CarrierFrame::decode(raw).unwrap().payload)
            .collect::<Vec<_>>();
        assert_eq!(rebuilt, data);
    }

    #[test]
    fn rejects_bad_fragment_count() {
        let frame = CarrierFrame {
            packet: Id128([1; 16]),
            index: 1,
            count: 1,
            payload: vec![],
        };
        assert!(frame.encode().is_err());
    }

    fn test_descriptor(address: &str, hydra_id: &str) -> GhostContactDescriptor {
        GhostContactDescriptor {
            version: 1,
            kaspa_address: address.to_owned(),
            hydra_contact_card_b64: "AQIDBA==".to_owned(),
            display_name: "Alice".to_owned(),
            hydra_identity_id: hydra_id.to_owned(),
            discoverable: false,
            username: String::new(),
            description: String::new(),
            interests: Vec::new(),
            avatar: None,
            capabilities: vec!["hydra-v1".to_owned()],
            expires_daa: None,
            signature_hex: "00".repeat(64),
        }
    }

    #[test]
    fn private_contact_request_roundtrips_and_binds_destination_field() {
        let request = GhostContactRequest {
            version: 1,
            request_id: "11".repeat(16),
            recipient_kaspa_address: "kaspatest:recipient".to_owned(),
            sender: test_descriptor("kaspatest:sender", &"22".repeat(32)),
            room_invite: None,
            call_invite: None,
            signature_hex: "33".repeat(64),
        };
        let encoded = request.encode().expect("encode GTCR");
        assert!(encoded.starts_with(&GTCR_MAGIC));
        let decoded = GhostContactRequest::decode(&encoded).expect("decode GTCR");
        assert_eq!(decoded.request_id, request.request_id);
        assert_eq!(
            decoded.recipient_kaspa_address,
            request.recipient_kaspa_address
        );
        assert_eq!(decoded.sender.kaspa_address, request.sender.kaspa_address);
        assert_eq!(
            decoded.sender.hydra_identity_id,
            request.sender.hydra_identity_id
        );
    }

    #[test]
    fn kktp_v2_discovery_and_response_are_canonical_and_share_sid() {
        let sid = "11".repeat(16);
        let request = GhostContactRequest {
            version: GHOST_KKTP_VERSION,
            request_id: sid.clone(),
            recipient_kaspa_address: "kaspatest:recipient".to_owned(),
            sender: test_descriptor("kaspatest:sender", &"22".repeat(32)),
            room_invite: None,
            call_invite: Some(GhostCallInviteContext {
                call_id: "44".repeat(16),
                action: "request".into(),
            }),
            signature_hex: "33".repeat(64),
        };
        let encoded = request.encode().expect("encode KKTP discovery");
        assert!(encoded.starts_with(KKTP_ANCHOR_PREFIX));
        assert_eq!(
            kktp_anchor_type(&encoded).unwrap().as_deref(),
            Some("discovery")
        );
        let decoded = GhostContactRequest::decode(&encoded).expect("decode KKTP discovery");
        assert_eq!(decoded.request_id, sid);
        assert_eq!(
            decoded
                .call_invite
                .as_ref()
                .map(|value| value.call_id.as_str()),
            Some("44444444444444444444444444444444")
        );
        assert_eq!(decoded.encode().unwrap(), encoded);

        let accepted = GhostContactAccept {
            version: GHOST_KKTP_VERSION,
            request_id: decoded.request_id.clone(),
            recipient_kaspa_address: decoded.sender.kaspa_address.clone(),
            acceptor_kaspa_address: decoded.recipient_kaspa_address.clone(),
            responder: test_descriptor("kaspatest:responder", &"44".repeat(32)),
            signature_hex: "55".repeat(64),
        };
        let response = accepted.encode().expect("encode KKTP response");
        assert_eq!(
            kktp_anchor_type(&response).unwrap().as_deref(),
            Some("response")
        );
        assert_eq!(
            GhostContactAccept::decode(&response).unwrap().request_id,
            decoded.request_id
        );
    }

    #[test]
    fn standalone_call_signal_is_canonical_and_independent_from_chat_sid() {
        let signal = GhostCallSignal {
            kind: "call_signal".into(),
            version: GHOST_KKTP_VERSION,
            signal_id: "aa".repeat(16),
            call_id: "bb".repeat(16),
            action: "request".into(),
            recipient_kaspa_address: "kaspatest:recipient".into(),
            sender: test_descriptor("kaspatest:sender", &"cc".repeat(32)),
            signature_hex: "dd".repeat(64),
        };
        let encoded = signal.encode().expect("encode call signal");
        assert_eq!(
            kktp_anchor_type(&encoded).unwrap().as_deref(),
            Some("call_signal")
        );
        let decoded = GhostCallSignal::decode(&encoded).expect("decode call signal");
        assert_eq!(decoded.signal_id, signal.signal_id);
        assert_eq!(decoded.call_id, signal.call_id);
        assert_eq!(decoded.action, "request");
        assert_eq!(decoded.encode().unwrap(), encoded);

        let mut invalid = decoded;
        invalid.action = "audio".into();
        assert!(invalid.encode().is_err());
    }

    #[test]
    fn kktp_session_end_is_canonical_and_dual_signature_bound() {
        let end = KktpSessionEnd {
            kind: "session_end".into(),
            version: GHOST_KKTP_VERSION,
            sid: "11".repeat(16),
            initiator_hydra_id: "22".repeat(32),
            responder_hydra_id: "33".repeat(32),
            sender_hydra_id: "22".repeat(32),
            sender_kaspa_address: "kaspatest:sender".into(),
            recipient_kaspa_address: "kaspatest:recipient".into(),
            reason: "left".into(),
            pq_sig_b64: "AQID".into(),
            sig: "44".repeat(64),
        };
        let encoded = end.encode().expect("encode KKTP session_end");
        assert_eq!(
            kktp_anchor_type(&encoded).unwrap().as_deref(),
            Some("session_end")
        );
        let decoded = KktpSessionEnd::decode(&encoded).expect("decode KKTP session_end");
        assert_eq!(decoded.sid, end.sid);
        assert_eq!(decoded.reason, "left");
        assert_eq!(decoded.encode().unwrap(), encoded);

        let mut wrong_sender = decoded.clone();
        wrong_sender.sender_hydra_id = "55".repeat(32);
        assert!(wrong_sender.encode().is_err());
    }

    #[test]
    fn kktp_mailbox_id_is_role_ordered_and_sid_bound() {
        let sid = "10".repeat(16);
        let a = "20".repeat(32);
        let b = "30".repeat(32);
        let ab = kktp_mailbox_id(&sid, &a, &b).unwrap();
        assert_eq!(ab.len(), 64);
        assert_ne!(ab, kktp_mailbox_id(&sid, &b, &a).unwrap());
        assert_ne!(ab, kktp_mailbox_id(&"11".repeat(16), &a, &b).unwrap());
    }

    #[test]
    fn kktp_mailbox_message_rejects_noncanonical_wire_json() {
        let wire = KktpMailboxMessage {
            kind: "msg".into(),
            version: GHOST_KKTP_VERSION,
            sid: "11".repeat(16),
            mailbox_id: "22".repeat(32),
            direction: KktpDirection::AtoB,
            seq: 0,
            sender_hydra_id: "33".repeat(32),
            message_id: "44".repeat(16),
            profile: "Off".into(),
            ciphertext_b64: "AQID".into(),
        };
        let canonical = wire.encode().unwrap();
        assert!(KktpMailboxMessage::decode(&canonical).is_ok());
        let text = std::str::from_utf8(&canonical).unwrap();
        let altered = text.replacen("{", "{ ", 1).into_bytes();
        assert!(KktpMailboxMessage::decode(&altered).is_err());
    }

    #[test]
    fn kktp_persistent_transport_seed_is_first_message_only_and_canonical() {
        let mut first = KktpInnerMessage::text(
            "11".repeat(16),
            "22".repeat(32),
            KktpDirection::AtoB,
            0,
            "33".repeat(16),
            "hello".into(),
        );
        first.resume_seed_b64 = Some(BASE64.encode([7u8; 32]));
        let encoded = first.encode().expect("encode seeded first message");
        let decoded = KktpInnerMessage::decode(&encoded).expect("decode seeded first message");
        assert_eq!(decoded.resume_seed_b64, first.resume_seed_b64);
        assert_eq!(decoded.encode().unwrap(), encoded);

        let mut later = first.clone();
        later.seq = 1;
        assert!(later.encode().is_err());

        let mut wrong_size = first;
        wrong_size.resume_seed_b64 = Some(BASE64.encode([9u8; 31]));
        assert!(wrong_size.encode().is_err());
    }
}
