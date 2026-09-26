//! Admission of p2p-net deliveries before any decryption is attempted.

use ghost_realtime::{session_topic, Gtr1Envelope, P2pIncoming, SessionPeerBindings};

/// Accept a delivery only when it is a GTR1 carrier for exactly this SID, on
/// this SID's topic, from the transport peer bound to the authenticated HYDRA
/// sender of that SID. Everything else is dropped silently.
pub(crate) fn admit(
    sid: &[u8; 16],
    bindings: &SessionPeerBindings,
    incoming: P2pIncoming,
) -> Option<P2pIncoming> {
    if incoming.topic != session_topic(sid) {
        return None;
    }
    let carrier = Gtr1Envelope::decode(&incoming.packet).ok()?;
    let bound = carrier.sid == *sid
        && bindings.verify_source(sid, &carrier.sender_hydra_id, &incoming.source_peer_id);
    bound.then_some(incoming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_realtime::SessionPeerBinding;

    const SID: [u8; 16] = [1; 16];
    const HYDRA: [u8; 32] = [2; 32];

    fn bindings() -> SessionPeerBindings {
        let mut bindings = SessionPeerBindings::default();
        bindings
            .bind(SessionPeerBinding {
                sid: SID,
                hydra_id: HYDRA,
                peer_id: "peer-a".into(),
            })
            .unwrap();
        bindings
    }

    fn incoming(sid: [u8; 16], sender: [u8; 32], source: &str, topic: String) -> P2pIncoming {
        let packet = Gtr1Envelope::new(sender, [3; 16], sid, vec![4])
            .unwrap()
            .encode()
            .unwrap();
        P2pIncoming {
            source_peer_id: source.into(),
            topic,
            packet,
        }
    }

    #[test]
    fn bound_carrier_on_its_topic_is_admitted() {
        let delivery = incoming(SID, HYDRA, "peer-a", session_topic(&SID));
        assert_eq!(admit(&SID, &bindings(), delivery.clone()), Some(delivery));
    }

    #[test]
    fn unbound_or_misrouted_deliveries_are_dropped() {
        let bindings = bindings();
        let wrong_source = incoming(SID, HYDRA, "peer-b", session_topic(&SID));
        let wrong_sender = incoming(SID, [9; 32], "peer-a", session_topic(&SID));
        let wrong_topic = incoming(SID, HYDRA, "peer-a", session_topic(&[7; 16]));
        let wrong_sid = incoming([7; 16], HYDRA, "peer-a", session_topic(&SID));
        for delivery in [wrong_source, wrong_sender, wrong_topic, wrong_sid] {
            assert_eq!(admit(&SID, &bindings, delivery), None);
        }
        let garbage = P2pIncoming {
            source_peer_id: "peer-a".into(),
            topic: session_topic(&SID),
            packet: b"not gtr1".to_vec(),
        };
        assert_eq!(admit(&SID, &bindings, garbage), None);
    }
}
