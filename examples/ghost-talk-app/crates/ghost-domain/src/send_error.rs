//! Send refusals that callers react to. Producers return these exact
//! messages and callers classify them through the functions below, so a
//! reworded message can never silently change how a send is handled.

pub const SESSION_ESTABLISHING: &str =
    "The secure session is still being established for a previous message";
pub const HANDSHAKE_PENDING: &str =
    "A secure-session handshake is already pending for this peer; retry after it completes";
pub const FINISH_AWAITING_ACK: &str = "The secure-session FINISH is still awaiting the peer's signed acknowledgement; wait for handshake completion before sending another message";
pub const RECOVERY_FINISH_AWAITING_BROADCAST: &str = "The secure-session recovery FINISH is still awaiting Kaspa broadcast; wait for handshake recovery before sending another message";
pub const INITIATOR_HANDSHAKE_PENDING: &str =
    "Waiting for the chat initiator's authenticated PQ handshake";
pub const NOT_ACCEPTED: &str = "The recipient has not accepted this Ghost Talk chat yet";
pub const UNKNOWN_HYDRA_PEER: &str = "KKTP message names an unknown HYDRA peer";
pub const NO_REALTIME_PEER: &str = "Realtime chat has no authenticated HYDRA peer.";
pub const INITIATOR_RESTORE_PENDING: &str =
    "Waiting for the original chat initiator to restore the encrypted peer transport after restart";
pub const TRANSPORT_RESTORING: &str =
    "Secure peer transport is restoring after restart; no new handshake was started";
pub const RESTORE_MISSING_SID: &str = "restart restore is missing the persisted KKTP SID";
pub const RESTORE_MISSING_ROLE: &str = "restart restore is missing the persisted KKTP role";

/// A handshake for this peer is already in flight: the message stays queued
/// and is sent when the session completes.
pub fn handshake_in_flight(error: &str) -> bool {
    [
        SESSION_ESTABLISHING,
        HANDSHAKE_PENDING,
        FINISH_AWAITING_ACK,
        RECOVERY_FINISH_AWAITING_BROADCAST,
    ]
    .contains(&error)
}

/// The existing transport will become usable without a new bootstrap.
pub fn waits_for_existing_transport(error: &str) -> bool {
    handshake_in_flight(error) || error == INITIATOR_HANDSHAKE_PENDING
}

/// There is no usable session with this peer; a fresh contact bootstrap is
/// required before realtime traffic can flow.
pub fn requires_contact_bootstrap(error: &str) -> bool {
    [
        NOT_ACCEPTED,
        UNKNOWN_HYDRA_PEER,
        NO_REALTIME_PEER,
        INITIATOR_RESTORE_PENDING,
        TRANSPORT_RESTORING,
        RESTORE_MISSING_SID,
        RESTORE_MISSING_ROLE,
    ]
    .contains(&error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_disjoint_and_exact() {
        assert!(handshake_in_flight(FINISH_AWAITING_ACK));
        assert!(!handshake_in_flight(INITIATOR_HANDSHAKE_PENDING));
        assert!(waits_for_existing_transport(INITIATOR_HANDSHAKE_PENDING));
        assert!(requires_contact_bootstrap(RESTORE_MISSING_ROLE));
        assert!(!requires_contact_bootstrap(HANDSHAKE_PENDING));
        assert!(!waits_for_existing_transport(NOT_ACCEPTED));
        assert!(!handshake_in_flight("network unreachable"));
    }
}
