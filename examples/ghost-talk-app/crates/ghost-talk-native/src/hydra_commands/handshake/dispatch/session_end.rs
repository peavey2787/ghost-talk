use super::{
    clear_peer_handshake_state, persist_transport_state, retire_kktp_binding, HydraMailboxResult,
    HydraProfileRuntime, HydraSessionEndedProjection, KktpRole, KktpSessionEnd, PeerRoute, BASE64,
};
use base64::Engine as _;

pub(crate) fn validate_session_end_recipient(
    runtime: &HydraProfileRuntime,
    end: &KktpSessionEnd,
    local_kaspa_addresses: &[String],
) -> Result<bool, String> {
    ghost_kaspa::verify_kktp_session_end(end)?;
    if !local_kaspa_addresses
        .iter()
        .any(|address| address == &end.recipient_kaspa_address)
    {
        return Err("KKTP session_end is not addressed to this Ghost Talk wallet".into());
    }
    Ok(end.sender_hydra_id != runtime.identity_id)
}

pub(crate) fn validate_session_end_participants(
    runtime: &HydraProfileRuntime,
    end: &KktpSessionEnd,
) -> Result<(), String> {
    let local_is_initiator = end.initiator_hydra_id == runtime.identity_id;
    let local_is_responder = end.responder_hydra_id == runtime.identity_id;
    if local_is_initiator == local_is_responder {
        return Err("KKTP session_end does not name this HYDRA identity exactly once".into());
    }
    let expected_peer = if local_is_initiator {
        &end.responder_hydra_id
    } else {
        &end.initiator_hydra_id
    };
    if expected_peer != &end.sender_hydra_id {
        return Err("KKTP session_end sender does not match the remote session participant".into());
    }
    Ok(())
}

pub(crate) fn verify_session_end_pq(
    runtime: &HydraProfileRuntime,
    end: &KktpSessionEnd,
) -> Result<(), String> {
    if !runtime.hydra.has_contact(&end.sender_hydra_id)? {
        return Err("KKTP session_end signer is not a known HYDRA peer".into());
    }
    let signature = BASE64
        .decode(&end.pq_sig_b64)
        .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
    runtime.hydra.verify_contact_application_context(
        &end.sender_hydra_id,
        &end.pq_signing_bytes()?,
        &signature,
    )
}

pub(crate) fn validate_session_end_binding(
    runtime: &HydraProfileRuntime,
    end: &KktpSessionEnd,
) -> Result<bool, String> {
    let Some(binding) = runtime.kktp_sessions.get(&end.sender_hydra_id) else {
        return Ok(false);
    };
    if binding.sid != end.sid {
        return Ok(false);
    }
    let local_initiated = binding.role == KktpRole::Initiator;
    let (expected_initiator, expected_responder) = if local_initiated {
        (runtime.identity_id.as_str(), end.sender_hydra_id.as_str())
    } else {
        (end.sender_hydra_id.as_str(), runtime.identity_id.as_str())
    };
    if end.initiator_hydra_id != expected_initiator || end.responder_hydra_id != expected_responder
    {
        return Err("KKTP session_end role ordering does not match the active binding".into());
    }
    Ok(true)
}

pub(crate) fn validate_session_end_route(
    runtime: &HydraProfileRuntime,
    end: &KktpSessionEnd,
) -> Result<(), String> {
    let Some(route) = runtime.peer_routes.get(&end.sender_hydra_id) else {
        return Ok(());
    };
    if route.kaspa_address == end.sender_kaspa_address {
        Ok(())
    } else {
        Err("KKTP session_end Kaspa signer does not match the authenticated peer route".into())
    }
}

pub(crate) fn apply_live_session_end(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
) -> Result<(), String> {
    match runtime.hydra.session_status(peer)?.as_str() {
        "active" => runtime.hydra.close_session(peer)?,
        "pending" => runtime.hydra.abort_handshake(peer)?,
        _ => {}
    }
    retire_kktp_binding(runtime, peer);
    clear_peer_handshake_state(runtime, peer);
    runtime.blocked_peers.insert(peer.to_owned());
    persist_transport_state(runtime)?;
    Ok(())
}

pub(crate) fn update_ended_peer_route(
    runtime: &mut HydraProfileRuntime,
    end: &KktpSessionEnd,
) -> String {
    let peer_label = runtime
        .peer_routes
        .get(&end.sender_hydra_id)
        .map(|route| route.display_name.clone())
        .unwrap_or_default();
    runtime.peer_routes.insert(
        end.sender_hydra_id.clone(),
        PeerRoute {
            kaspa_address: end.sender_kaspa_address.clone(),
            display_name: peer_label.clone(),
            session_sid: None,
            session_role: None,
            resume_required: false,
        },
    );
    peer_label
}

pub(crate) fn project_session_end(end: KktpSessionEnd, peer_label: String) -> HydraMailboxResult {
    HydraMailboxResult {
        peer_address: Some(end.sender_kaspa_address.clone()),
        peer_label: (!peer_label.is_empty()).then_some(peer_label),
        session_ended: Some(HydraSessionEndedProjection {
            peer_hydra_id: end.sender_hydra_id,
            peer_address: end.sender_kaspa_address,
            sid: end.sid,
            reason: end.reason,
        }),
        ..discard_result()
    }
}
use super::super::super::mailbox_dispatch::discard_result;
