use super::super::{
    envelope_open::open_kktp_envelope,
    handshake_admission::{authenticate_handshake, handle_pq_init},
    handshake_response::{handle_pq_finish, handle_pq_response},
    mailbox_dispatch::discard_result,
    runtime_session_queries::{clear_peer_handshake_state, kktp_sid_is_current},
    runtime_state::HydraProfileRuntime,
    session_state::{install_kktp_binding, retire_kktp_binding},
    session_types::{
        fresh_kktp_sid, HydraMailboxResult, HydraRecoveryProjection, HydraSessionEndedProjection,
        KktpHandshakeControl, KktpMailboxMessage, KktpRole, KktpSessionBinding, KktpSessionEnd,
        KktpSessionState, PeerRoute, PendingRecovery, ReceivedProjection, BASE64,
    },
    transport_persistence::persist_transport_state,
};
pub(crate) fn handle_kktp_handshake(
    runtime: &mut HydraProfileRuntime,
    control: KktpHandshakeControl,
) -> Result<HydraMailboxResult, String> {
    let Some(auth) = authenticate_handshake(runtime, &control)? else {
        return Ok(discard_result());
    };
    if control.stage == "pq_init" {
        return handle_pq_init(runtime, &control, auth);
    }
    if control.stage == "pq_resp" {
        return handle_pq_response(runtime, &control, auth);
    }
    if control.stage == "pq_finish" {
        return handle_pq_finish(runtime, &control, auth);
    }
    Err("unknown Ghost Talk KKTP PQ handshake stage".into())
}

pub(crate) fn routed_mailbox_result(
    route: Option<&PeerRoute>,
    message_id: String,
    discard: bool,
) -> HydraMailboxResult {
    HydraMailboxResult {
        peer_address: route.map(|value| value.kaspa_address.clone()),
        peer_label: route.map(|value| value.display_name.clone()),
        message_id: Some(message_id),
        discard,
        ..discard_result()
    }
}

pub(crate) fn waiting_mailbox_result(route: &PeerRoute, message_id: String) -> HydraMailboxResult {
    routed_mailbox_result(Some(route), message_id, false)
}

pub(crate) fn prepare_kktp_recovery(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    route: &PeerRoute,
    message_id: String,
) -> Result<HydraMailboxResult, String> {
    let sid = fresh_kktp_sid();
    runtime.hydra.abort_handshake(peer)?;
    let offer = runtime.hydra.init_handshake(peer)?;
    install_kktp_binding(
        runtime,
        peer,
        sid.clone(),
        KktpRole::Initiator,
        KktpSessionState::Handshake,
    )?;
    let offer_hex = hex::encode(&offer);
    runtime.pending_recovery.insert(
        peer.to_owned(),
        PendingRecovery {
            sid: sid.clone(),
            contact_id: peer.to_owned(),
            destination: route.kaspa_address.clone(),
            offer_hex: offer_hex.clone(),
            offer_broadcast: false,
        },
    );
    Ok(HydraMailboxResult {
        recovery: Some(HydraRecoveryProjection {
            destination: route.kaspa_address.clone(),
            offer_hex,
            sid,
            peer_hydra_id: peer.to_owned(),
        }),
        peer_address: Some(route.kaspa_address.clone()),
        peer_label: Some(route.display_name.clone()),
        message_id: Some(message_id),
        discard: false,
        ..discard_result()
    })
}

pub(crate) fn handle_missing_mailbox_binding(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    route: Option<PeerRoute>,
    message_id: String,
) -> Result<HydraMailboxResult, String> {
    if !runtime.hydra.has_contact(peer)? {
        return Err("KKTP message names an unknown HYDRA peer".into());
    }
    let route = route.ok_or_else(|| "KKTP message has no verified Kaspa peer route".to_string())?;
    if route.resume_required {
        return Ok(waiting_mailbox_result(&route, message_id));
    }
    if runtime.hydra.session_status(peer)? == "pending" {
        return Ok(waiting_mailbox_result(&route, message_id));
    }
    prepare_kktp_recovery(runtime, peer, &route, message_id)
}

pub(crate) fn mailbox_message_ready(
    binding: &KktpSessionBinding,
    wire: &KktpMailboxMessage,
) -> Option<bool> {
    if wire.seq < binding.recv_next_seq {
        Some(false)
    } else if wire.seq > binding.recv_next_seq || binding.state != KktpSessionState::Active {
        None
    } else {
        Some(true)
    }
}

pub(crate) fn decrypt_mailbox_wire(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    wire: &KktpMailboxMessage,
) -> Result<Option<ReceivedProjection>, String> {
    let encrypted = BASE64
        .decode(&wire.ciphertext_b64)
        .map_err(|_| "KKTP HYDRA ciphertext is not valid base64".to_string())?;
    open_kktp_envelope(
        runtime,
        peer,
        &wire.sid,
        &wire.mailbox_id,
        wire.direction,
        wire.seq,
        &wire.message_id,
        &wire.profile,
        &encrypted,
    )
}

pub(crate) fn handle_kktp_mailbox_message(
    runtime: &mut HydraProfileRuntime,
    wire: KktpMailboxMessage,
) -> Result<HydraMailboxResult, String> {
    let peer = wire.sender_hydra_id.clone();
    let route = runtime.peer_routes.get(&peer).cloned();
    if let Some(result) = mailbox_preflight(runtime, &wire, route.as_ref()) {
        return Ok(result);
    }
    let Some(binding) = runtime.kktp_sessions.get(&peer).cloned() else {
        return handle_missing_mailbox_binding(runtime, &peer, route, wire.message_id);
    };
    if binding.sid != wire.sid {
        return Ok(routed_mailbox_result(route.as_ref(), wire.message_id, true));
    }
    if let Some(result) = unready_mailbox_result(&binding, &wire, route.as_ref()) {
        return Ok(result);
    }
    let received = decrypt_mailbox_wire(runtime, &peer, &wire)?;
    let route = runtime.peer_routes.get(&peer);
    Ok(HydraMailboxResult {
        received,
        peer_address: route.map(|value| value.kaspa_address.clone()),
        peer_label: route.map(|value| value.display_name.clone()),
        message_id: Some(wire.message_id),
        ..discard_result()
    })
}

fn mailbox_preflight(
    runtime: &HydraProfileRuntime,
    wire: &KktpMailboxMessage,
    route: Option<&PeerRoute>,
) -> Option<HydraMailboxResult> {
    if !kktp_sid_is_current(runtime, &wire.sender_hydra_id, &wire.sid) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-message-discarded",
            format!(
                "sid={} peer={} message={}",
                wire.sid, wire.sender_hydra_id, wire.message_id
            ),
        );
        return Some(discard_result());
    }
    if runtime.blocked_peers.contains(&wire.sender_hydra_id) {
        return Some(routed_mailbox_result(route, wire.message_id.clone(), true));
    }
    None
}

fn unready_mailbox_result(
    binding: &KktpSessionBinding,
    wire: &KktpMailboxMessage,
    route: Option<&PeerRoute>,
) -> Option<HydraMailboxResult> {
    let readiness = mailbox_message_ready(binding, wire);
    (readiness != Some(true)).then(|| {
        if readiness == Some(false) {
            discard_result()
        } else {
            routed_mailbox_result(route, wire.message_id.clone(), false)
        }
    })
}

mod session_end;
use base64::Engine as _;
pub(crate) use session_end::{
    apply_live_session_end, project_session_end, update_ended_peer_route,
    validate_session_end_binding, validate_session_end_participants,
    validate_session_end_recipient, validate_session_end_route, verify_session_end_pq,
};
