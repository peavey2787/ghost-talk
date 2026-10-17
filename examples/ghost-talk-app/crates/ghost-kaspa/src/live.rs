use sha2::{Digest, Sha256};

/// Deterministic fallback id for a Ghost carrier when a live node omits tx verbose data.
pub fn fallback_live_event_id(block_hash: &str, tx_index: usize, payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/live-block-event/v1\0");
    hasher.update(block_hash.as_bytes());
    hasher.update((tx_index as u64).to_le_bytes());
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

/// The mailbox event both hosts emit for one Kaspa-carried live observation.
pub fn live_mailbox_event(
    observation: ghost_core::TransactionObservation,
) -> ghost_api::MailboxEvent {
    ghost_api::MailboxEvent {
        transaction_id: observation.txid,
        blue_score: observation.daa_score.to_string(),
        payload_hex: hex::encode(observation.payload),
        block_time: None,
        via_swarm: false,
    }
}

/// Return true only for payloads owned by Ghost Talk/KasKold transport protocols.
pub fn is_live_ghost_carrier(payload: &[u8]) -> bool {
    payload.starts_with(b"KKTP:")
        || (payload.len() >= 4
            && matches!(
                &payload[..4],
                b"GHST"
                    | b"GTCD"
                    | b"GTCR"
                    | b"GTCA"
                    | b"GTAK"
                    | b"GTVA"
                    | b"GTVL"
                    | b"GTBK"
                    | b"GTR1"
                    | b"GTRA"
            ))
}

#[cfg(test)]
mod tests {
    use super::{is_live_ghost_carrier, live_mailbox_event};

    #[test]
    fn live_observations_become_kaspa_carried_mailbox_events() {
        let event = live_mailbox_event(ghost_core::TransactionObservation {
            txid: "ab".into(),
            daa_score: 7,
            payload: b"GHST".to_vec(),
        });
        assert_eq!(event.blue_score, "7");
        assert_eq!(event.payload_hex, "47485354");
        assert!(!event.via_swarm && event.block_time.is_none());
    }

    #[test]
    fn realtime_and_mailbox_carriers_are_live_but_other_payloads_are_not() {
        for carrier in [
            &b"GTR1...."[..],
            b"KKTP:ANCHOR:{}",
            b"GHST....",
            b"GTCR....",
            b"GTRA{}",
        ] {
            assert!(is_live_ghost_carrier(carrier));
        }
        for other in [&b""[..], b"GTR", b"hello", b"GTR2...."] {
            assert!(!is_live_ghost_carrier(other));
        }
    }
}
