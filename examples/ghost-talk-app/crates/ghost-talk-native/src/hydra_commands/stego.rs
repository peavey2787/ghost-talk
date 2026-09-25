#[cfg(test)]
mod tests {
    use super::super::delivery_state::{
        delivery_ack_matches_prepared_completion, PreparedCompletion,
    };
    use super::super::session_types::restart_successor_sid;

    fn prepared_completion(peer: &str, message_id: &str) -> PreparedCompletion {
        PreparedCompletion {
            pending_id: "pending".into(),
            message_id: message_id.into(),
            sid: "00112233445566778899aabbccddeeff".into(),
            contact_id: peer.into(),
            destination: "kaspa:example".into(),
            payloads_hex: vec!["00".into()],
        }
    }

    fn delivery_ack(peer: &str, message_id: &str) -> ghost_protocol::GhostDeliveryAck {
        ghost_protocol::GhostDeliveryAck {
            version: 1,
            signer_kaspa_address: "kaspa:example".into(),
            signer_hydra_id: peer.into(),
            destination_hydra_id: "local".into(),
            message_id: message_id.into(),
            signature_hex: String::new(),
        }
    }

    #[test]
    fn restart_successor_sid_is_deterministic_and_role_ordered() {
        let prior = "00112233445566778899aabbccddeeff";
        let initiator = "11".repeat(32);
        let responder = "22".repeat(32);
        let successor = restart_successor_sid(prior, &initiator, &responder).unwrap();
        assert_eq!(successor, "ce15bb1771600ee6b6d66f53bc63d508");
        assert_eq!(
            successor,
            restart_successor_sid(prior, &initiator, &responder).unwrap()
        );
        assert_ne!(
            successor,
            restart_successor_sid(prior, &responder, &initiator).unwrap()
        );
    }

    #[test]
    fn delivery_ack_completion_match_is_exact() {
        let peer = "a".repeat(64);
        let other_peer = "b".repeat(64);
        let message_id = "1".repeat(32);
        let old_message_id = "2".repeat(32);
        let prepared = prepared_completion(&peer, &message_id);

        assert!(delivery_ack_matches_prepared_completion(
            Some(&prepared),
            &delivery_ack(&peer, &message_id),
        ));
        assert!(!delivery_ack_matches_prepared_completion(
            Some(&prepared),
            &delivery_ack(&peer, &old_message_id),
        ));
        assert!(!delivery_ack_matches_prepared_completion(
            Some(&prepared),
            &delivery_ack(&other_peer, &message_id),
        ));
        assert!(!delivery_ack_matches_prepared_completion(
            None,
            &delivery_ack(&peer, &message_id),
        ));
    }
}
