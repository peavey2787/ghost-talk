use super::super::runtime_state::HydraProfileRuntime;

pub(crate) fn kktp_sid_is_current(runtime: &HydraProfileRuntime, peer: &str, sid: &str) -> bool {
    allowed_sid_matches(runtime, peer, sid)
        || established_sid_matches(runtime, peer, sid)
        || pending_sid_matches(runtime, peer, sid)
        || prepared_sid_matches(runtime, peer, sid)
        || route_sid_matches(runtime, peer, sid)
}

pub(crate) fn allowed_sid_matches(runtime: &HydraProfileRuntime, peer: &str, sid: &str) -> bool {
    runtime
        .allowed_kktp_sids
        .get(peer)
        .is_some_and(|sids| sids.contains(&sid.to_ascii_lowercase()))
}

pub(crate) fn established_sid_matches(
    runtime: &HydraProfileRuntime,
    peer: &str,
    sid: &str,
) -> bool {
    runtime
        .kktp_sessions
        .get(peer)
        .is_some_and(|binding| binding.sid == sid)
}

pub(crate) fn pending_sid_matches(runtime: &HydraProfileRuntime, peer: &str, sid: &str) -> bool {
    runtime
        .pending_outbound
        .get(peer)
        .is_some_and(|value| value.sid == sid)
        || runtime
            .pending_inbound
            .get(peer)
            .is_some_and(|value| value.sid == sid)
        || runtime
            .pending_recovery
            .get(peer)
            .is_some_and(|value| value.sid == sid)
}

pub(crate) fn prepared_sid_matches(runtime: &HydraProfileRuntime, peer: &str, sid: &str) -> bool {
    runtime
        .prepared_completion
        .get(peer)
        .is_some_and(|value| value.sid == sid)
        || runtime
            .prepared_recovery_finish
            .get(peer)
            .is_some_and(|value| value.sid == sid)
}

pub(crate) fn route_sid_matches(runtime: &HydraProfileRuntime, peer: &str, sid: &str) -> bool {
    runtime
        .peer_routes
        .get(peer)
        .and_then(|route| route.session_sid())
        == Some(sid)
}

pub(crate) fn clear_peer_handshake_state(runtime: &mut HydraProfileRuntime, contact_id: &str) {
    runtime.pending_outbound.remove(contact_id);
    runtime.prepared_completion.remove(contact_id);
    runtime.pending_recovery.remove(contact_id);
    runtime.prepared_recovery_finish.remove(contact_id);
    runtime.pending_inbound.remove(contact_id);
    runtime
        .prepared_kktp_deliveries
        .retain(|_, prepared| prepared.contact_id != contact_id);
}
