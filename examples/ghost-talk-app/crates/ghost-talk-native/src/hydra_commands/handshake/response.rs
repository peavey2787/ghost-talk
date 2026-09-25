use super::super::session_message_delivery::kktp_first_message;
use super::super::{
    delivery_state::PreparedCompletion,
    envelope_open::open_kktp_envelope,
    handshake_admission::{
        control_mailbox_result, replay_prepared_completion, replay_prepared_recovery,
        valid_initiator_binding,
    },
    handshake_restart_rules::AuthenticatedHandshake,
    mailbox_dispatch::{discard_result, frame_control},
    runtime_state::HydraProfileRuntime,
    session_state::{kktp_handshake_payload, mark_peer_route_session_active},
    session_types::{
        HydraMailboxResult, KktpFirstMessage, KktpHandshakeControl, KktpRole, KktpSessionBinding,
        KktpSessionState, PeerRoute, PreparedRecoveryFinish, ReceivedProjection, BASE64,
    },
};
pub(crate) fn finish_pq_response(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    sid: &str,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    match runtime.hydra.finish_handshake(payload) {
        Ok(finish) => Ok(Some(finish)),
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "handshake",
                "obsolete-pq-response-discarded",
                format!("sid={sid} peer={peer} error={error}"),
            );
            Ok(None)
        }
    }
}

pub(crate) fn prepare_first_message_finish(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    sid: &str,
    finish: &[u8],
) -> Result<Option<HydraMailboxResult>, String> {
    let Some(pending) = runtime.pending_outbound.get(peer).cloned() else {
        return Ok(None);
    };
    if pending.sid != sid {
        return Ok(None);
    }
    let first = kktp_first_message(
        runtime,
        peer,
        &pending.message_id,
        &pending.body,
        &pending.stego_profile,
    )?;
    let binding = runtime
        .kktp_sessions
        .get(peer)
        .cloned()
        .ok_or_else(|| "KKTP initiator binding disappeared".to_string())?;
    let finish_wire = kktp_handshake_payload(runtime, &binding, "pq_finish", finish, Some(first))?;
    let payloads_hex = frame_control(finish_wire)?.payloads_hex;
    runtime.prepared_completion.insert(
        peer.to_owned(),
        PreparedCompletion {
            pending_id: pending.id.clone(),
            message_id: pending.message_id.clone(),
            sid: sid.to_owned(),
            contact_id: peer.to_owned(),
            destination: pending.destination.clone(),
            payloads_hex: payloads_hex.clone(),
        },
    );
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-finish-prepared-with-first-message",
        format!(
            "sid={} peer={} pending={} message={}",
            sid, peer, pending.id, pending.message_id
        ),
    );
    Ok(Some(control_mailbox_result(
        pending.destination,
        payloads_hex,
        Some(pending.id),
        None,
    )))
}

pub(crate) fn prepare_recovery_finish(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    sid: &str,
    finish: &[u8],
) -> Result<Option<HydraMailboxResult>, String> {
    let Some(pending) = runtime.pending_recovery.get(peer).cloned() else {
        return Ok(None);
    };
    if pending.sid != sid {
        return Ok(None);
    }
    let binding = runtime
        .kktp_sessions
        .get(peer)
        .cloned()
        .ok_or_else(|| "KKTP recovery binding disappeared".to_string())?;
    let finish_wire = kktp_handshake_payload(runtime, &binding, "pq_finish", finish, None)?;
    let payloads_hex = frame_control(finish_wire)?.payloads_hex;
    runtime.prepared_recovery_finish.insert(
        peer.to_owned(),
        PreparedRecoveryFinish {
            sid: sid.to_owned(),
            contact_id: peer.to_owned(),
            destination: pending.destination.clone(),
            payloads_hex: payloads_hex.clone(),
        },
    );
    Ok(Some(control_mailbox_result(
        pending.destination,
        payloads_hex,
        None,
        Some(peer.to_owned()),
    )))
}

pub(crate) fn handle_pq_response(
    runtime: &mut HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: AuthenticatedHandshake,
) -> Result<HydraMailboxResult, String> {
    validate_response_role(&auth)?;
    if !response_binding_exists(runtime, control, &auth) {
        return Ok(discard_result());
    }
    if let Some(result) = replay_response(runtime, control, &auth) {
        return Ok(result);
    }
    let Some(finish) = finish_pq_response(runtime, &auth.peer, &control.sid, &auth.payload)? else {
        return Ok(discard_result());
    };
    activate_response_session(runtime, control, &auth);
    prepare_response_finish(runtime, control, &auth, &finish)
}

fn validate_response_role(auth: &AuthenticatedHandshake) -> Result<(), String> {
    if auth.local_role != KktpRole::Initiator {
        return Err("KKTP pq_resp role mapping is invalid".into());
    }
    Ok(())
}

fn response_binding_exists(
    runtime: &HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: &AuthenticatedHandshake,
) -> bool {
    if valid_initiator_binding(runtime, &auth.peer, &control.sid).is_some() {
        return true;
    }
    crate::debug_log::record(
        "warn",
        "handshake",
        "orphan-pq-response-discarded",
        format!("sid={} peer={}", control.sid, auth.peer),
    );
    false
}

fn replay_response(
    runtime: &HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: &AuthenticatedHandshake,
) -> Option<HydraMailboxResult> {
    replay_prepared_completion(runtime, &auth.peer, &control.sid)
        .or_else(|| replay_prepared_recovery(runtime, &auth.peer, &control.sid))
}

fn activate_response_session(
    runtime: &mut HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: &AuthenticatedHandshake,
) {
    if let Some(current) = runtime.kktp_sessions.get_mut(&auth.peer) {
        current.state = KktpSessionState::Active;
    }
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-response-applied-finish-preparing",
        format!(
            "sid={} peer={} initiator_state=Active-awaiting-finish-ack",
            control.sid, auth.peer
        ),
    );
}

fn prepare_response_finish(
    runtime: &mut HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: &AuthenticatedHandshake,
    finish: &[u8],
) -> Result<HydraMailboxResult, String> {
    if let Some(result) = prepare_first_message_finish(runtime, &auth.peer, &control.sid, finish)? {
        return Ok(result);
    }
    if let Some(result) = prepare_recovery_finish(runtime, &auth.peer, &control.sid, finish)? {
        return Ok(result);
    }
    Ok(discard_result())
}

mod finish;
pub(crate) use finish::handle_pq_finish;
