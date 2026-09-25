use super::{
    handshake_expected_sender, handshake_local_peer, restart_handshake_sid_allowed,
    AuthenticatedHandshake, HydraProfileRuntime, KktpHandshakeControl, KktpRole, PeerRoute, BASE64,
};
use base64::Engine as _;

fn handshake_sid_allowed(
    runtime: &HydraProfileRuntime,
    peer: &str,
    route: &PeerRoute,
    control: &KktpHandshakeControl,
    local_role: KktpRole,
) -> bool {
    restart_handshake_sid_allowed(route, control, local_role)
        || super::kktp_sid_is_current(runtime, peer, &control.sid)
}

fn verify_handshake_signature(
    runtime: &HydraProfileRuntime,
    peer: &str,
    control: &KktpHandshakeControl,
) -> Result<(), String> {
    let signature = BASE64
        .decode(&control.pq_sig_b64)
        .map_err(|_| "KKTP PQ context signature is not valid base64".to_string())?;
    runtime
        .hydra
        .verify_contact_application_context(peer, &control.signing_bytes()?, &signature)
}

pub(crate) fn authenticate_handshake(
    runtime: &HydraProfileRuntime,
    control: &KktpHandshakeControl,
) -> Result<Option<AuthenticatedHandshake>, String> {
    let local = runtime.identity_id.as_str();
    let (peer, local_role) = handshake_local_peer(local, control)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-control-received",
        format!(
            "stage={} sid={} local_role={:?} peer={}",
            control.stage, control.sid, local_role, peer
        ),
    );
    if discard_blocked_handshake(runtime, control, &peer) {
        return Ok(None);
    }
    let expected_sender = handshake_expected_sender(control)?;
    if expected_sender == local {
        return Ok(None);
    }
    validate_remote_handshake_sender(runtime, &peer, expected_sender)?;
    let route = runtime
        .peer_routes
        .get(&peer)
        .cloned()
        .ok_or_else(|| "KKTP PQ handshake has no verified Kaspa peer route".to_string())?;
    if discard_stale_handshake(runtime, &peer, &route, control, local_role) {
        return Ok(None);
    }
    verify_handshake_signature(runtime, &peer, control)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-control-authenticated",
        format!("stage={} sid={} peer={}", control.stage, control.sid, peer),
    );
    let payload = BASE64
        .decode(&control.payload_b64)
        .map_err(|_| "KKTP PQ handshake payload is not valid base64".to_string())?;
    Ok(Some(AuthenticatedHandshake {
        peer,
        local_role,
        route,
        payload,
    }))
}

fn discard_blocked_handshake(
    runtime: &HydraProfileRuntime,
    control: &KktpHandshakeControl,
    peer: &str,
) -> bool {
    let blocked = runtime.blocked_peers.contains(peer);
    if blocked {
        crate::debug_log::record(
            "warn",
            "handshake",
            "pq-control-blocked-peer",
            format!("stage={} sid={} peer={peer}", control.stage, control.sid),
        );
    }
    blocked
}

fn validate_remote_handshake_sender(
    runtime: &HydraProfileRuntime,
    peer: &str,
    expected_sender: &str,
) -> Result<(), String> {
    if expected_sender != peer {
        return Err("KKTP PQ handshake sender role is inconsistent".into());
    }
    if !runtime.hydra.has_contact(peer)? {
        return Err("KKTP PQ handshake names an unknown HYDRA contact".into());
    }
    Ok(())
}

fn discard_stale_handshake(
    runtime: &HydraProfileRuntime,
    peer: &str,
    route: &PeerRoute,
    control: &KktpHandshakeControl,
    local_role: KktpRole,
) -> bool {
    let stale = !handshake_sid_allowed(runtime, peer, route, control, local_role);
    if stale {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-handshake-discarded",
            format!("stage={} sid={} peer={peer}", control.stage, control.sid),
        );
    }
    stale
}
