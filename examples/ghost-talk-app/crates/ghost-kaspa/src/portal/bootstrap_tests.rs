use super::contact_signatures::{
    sign_call_signal, sign_contact_accept, sign_contact_request, sign_gtcd, sign_kktp_session_end,
    verify_call_signal, verify_contact_accept, verify_contact_request, verify_kktp_session_end,
};
#[cfg(all(test, feature = "upstream"))]
mod contact_bootstrap_tests {
    use super::*;

    fn address_for(private_key: &[u8; 32]) -> String {
        let xonly = kaspa_portal::wallet::derivation::bip32::pubkey_from_raw_key(private_key)
            .expect("valid test key");
        kaspa_portal::primitives::address::encode_p2pk_address(&xonly, "kaspatest")
    }

    fn signed_descriptor(
        private_key: &[u8; 32],
        hydra_byte: u8,
    ) -> ghost_protocol::GhostContactDescriptor {
        let mut descriptor = ghost_protocol::GhostContactDescriptor {
            version: 1,
            kaspa_address: address_for(private_key),
            hydra_contact_card_b64: "AQIDBA==".to_owned(),
            display_name: "test peer".to_owned(),
            hydra_identity_id: hex::encode([hydra_byte; 32]),
            discoverable: false,
            username: String::new(),
            description: String::new(),
            interests: Vec::new(),
            avatar: None,
            capabilities: vec!["hydra-v1".to_owned()],
            expires_daa: None,
            signature_hex: String::new(),
        };
        sign_gtcd(&mut descriptor, private_key).expect("sign GTCD");
        descriptor
    }

    #[test]
    fn contact_request_signature_binds_recipient_destination() {
        let sender_key = [1u8; 32];
        let mut request = ghost_protocol::GhostContactRequest {
            version: 1,
            request_id: "12".repeat(16),
            recipient_kaspa_address: address_for(&[2u8; 32]),
            sender: signed_descriptor(&sender_key, 3),
            room_invite: None,
            call_invite: None,
            signature_hex: String::new(),
        };
        sign_contact_request(&mut request, &sender_key).expect("sign request");
        verify_contact_request(&request).expect("verify request");
        request.recipient_kaspa_address = address_for(&[4u8; 32]);
        assert!(verify_contact_request(&request).is_err());
    }

    #[test]
    fn contact_request_signature_binds_room_invite_context() {
        let sender_key = [5u8; 32];
        let mut request = ghost_protocol::GhostContactRequest {
            version: ghost_protocol::GHOST_KKTP_VERSION,
            request_id: "34".repeat(16),
            recipient_kaspa_address: address_for(&[6u8; 32]),
            sender: signed_descriptor(&sender_key, 7),
            room_invite: Some(ghost_protocol::GhostRoomInviteContext {
                room_id: "89".repeat(16),
                room_name: "Test Room".into(),
            }),
            call_invite: None,
            signature_hex: String::new(),
        };
        sign_contact_request(&mut request, &sender_key).expect("sign request");
        verify_contact_request(&request).expect("verify request");
        request.room_invite.as_mut().unwrap().room_name = "Changed Room".into();
        assert!(verify_contact_request(&request).is_err());
    }

    #[test]
    fn contact_request_signature_binds_call_invite_context() {
        let sender_key = [10u8; 32];
        let mut request = ghost_protocol::GhostContactRequest {
            version: ghost_protocol::GHOST_KKTP_VERSION,
            request_id: "56".repeat(16),
            recipient_kaspa_address: address_for(&[11u8; 32]),
            sender: signed_descriptor(&sender_key, 12),
            room_invite: None,
            call_invite: Some(ghost_protocol::GhostCallInviteContext {
                call_id: "78".repeat(16),
                action: "request".into(),
            }),
            signature_hex: String::new(),
        };
        sign_contact_request(&mut request, &sender_key).expect("sign request");
        verify_contact_request(&request).expect("verify request");
        let mut wrong_call = request.clone();
        wrong_call.call_invite.as_mut().unwrap().call_id = "79".repeat(16);
        assert!(verify_contact_request(&wrong_call).is_err());
        request.call_invite.as_mut().unwrap().action = "decline".into();
        assert!(verify_contact_request(&request).is_err());
    }

    #[test]
    fn call_signal_signature_binds_call_action_and_recipient() {
        let sender_key = [31u8; 32];
        let mut signal = ghost_protocol::GhostCallSignal {
            kind: "call_signal".into(),
            version: ghost_protocol::GHOST_KKTP_VERSION,
            signal_id: "ab".repeat(16),
            call_id: "cd".repeat(16),
            action: "request".into(),
            recipient_kaspa_address: address_for(&[32u8; 32]),
            sender: signed_descriptor(&sender_key, 33),
            signature_hex: String::new(),
        };
        sign_call_signal(&mut signal, &sender_key).expect("sign call signal");
        verify_call_signal(&signal).expect("verify call signal");

        let mut wrong_action = signal.clone();
        wrong_action.action = "decline".into();
        assert!(verify_call_signal(&wrong_action).is_err());

        let mut wrong_recipient = signal;
        wrong_recipient.recipient_kaspa_address = address_for(&[34u8; 32]);
        assert!(verify_call_signal(&wrong_recipient).is_err());
    }

    #[test]
    fn kktp_session_end_signature_binds_sid_sender_and_destination() {
        let sender_key = [21u8; 32];
        let recipient_key = [22u8; 32];
        let sender_address = address_for(&sender_key);
        let recipient_address = address_for(&recipient_key);
        let mut end = ghost_protocol::KktpSessionEnd {
            kind: "session_end".into(),
            version: ghost_protocol::GHOST_KKTP_VERSION,
            sid: "11".repeat(16),
            initiator_hydra_id: "22".repeat(32),
            responder_hydra_id: "33".repeat(32),
            sender_hydra_id: "22".repeat(32),
            sender_kaspa_address: sender_address,
            recipient_kaspa_address: recipient_address,
            reason: "left".into(),
            pq_sig_b64: "AQID".into(),
            sig: String::new(),
        };
        sign_kktp_session_end(&mut end, &sender_key).expect("sign session_end");
        verify_kktp_session_end(&end).expect("verify session_end");

        let mut wrong_sid = end.clone();
        wrong_sid.sid = "44".repeat(16);
        assert!(verify_kktp_session_end(&wrong_sid).is_err());
        let mut wrong_destination = end.clone();
        wrong_destination.recipient_kaspa_address = address_for(&[23u8; 32]);
        assert!(verify_kktp_session_end(&wrong_destination).is_err());
    }

    #[test]
    fn contact_accept_signature_binds_exact_acceptor_and_return_destination() {
        let responder_key = [5u8; 32];
        let mut accepted = ghost_protocol::GhostContactAccept {
            version: ghost_protocol::GHOST_KKTP_VERSION,
            request_id: "34".repeat(16),
            recipient_kaspa_address: address_for(&[6u8; 32]),
            acceptor_kaspa_address: address_for(&responder_key),
            responder: signed_descriptor(&responder_key, 7),
            signature_hex: String::new(),
        };
        sign_contact_accept(&mut accepted, &responder_key).expect("sign acceptance");
        verify_contact_accept(&accepted).expect("verify acceptance");

        let mut wrong_acceptor = accepted.clone();
        wrong_acceptor.acceptor_kaspa_address = address_for(&[8u8; 32]);
        assert!(verify_contact_accept(&wrong_acceptor).is_err());

        let mut wrong_destination = accepted;
        wrong_destination.recipient_kaspa_address = address_for(&[9u8; 32]);
        assert!(verify_contact_accept(&wrong_destination).is_err());
    }
}
