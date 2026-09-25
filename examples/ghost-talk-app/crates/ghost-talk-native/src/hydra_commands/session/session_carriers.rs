use super::super::{
    contact_acceptance::{decode_contact_card, validate_contact_request_recipient},
    handshake_dispatch::{
        apply_live_session_end, project_session_end, update_ended_peer_route,
        validate_session_end_binding, validate_session_end_participants,
        validate_session_end_recipient, validate_session_end_route, verify_session_end_pq,
    },
    mailbox_dispatch::{decode_fixed_hex, discard_result},
    runtime_session_queries::kktp_sid_is_current,
    runtime_state::HydraProfileRuntime,
    session_state::remember_retired_kktp_sid,
    session_types::{
        kktp_anchor_type, ActiveSessionRegistration, HydraCallSignalProjection,
        HydraIncomingRequestProjection, HydraMailboxResult, HydraRoomInviteProjection,
        KktpSessionEnd, BASE64, KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX, MAX_CONTACT_CARD_BYTES,
        MAX_MAILBOX_ENVELOPE_BYTES,
    },
};
use std::collections::{HashMap, HashSet};
pub(crate) fn handle_kktp_session_end(
    runtime: &mut HydraProfileRuntime,
    end: KktpSessionEnd,
    local_kaspa_addresses: &[String],
) -> Result<HydraMailboxResult, String> {
    if !validate_session_end_recipient(runtime, &end, local_kaspa_addresses)? {
        return Ok(discard_result());
    }
    validate_session_end_participants(runtime, &end)?;
    if !kktp_sid_is_current(runtime, &end.sender_hydra_id, &end.sid) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-session-end-discarded",
            format!("sid={} peer={}", end.sid, end.sender_hydra_id),
        );
        return Ok(discard_result());
    }
    verify_session_end_pq(runtime, &end)?;
    if runtime.retired_kktp_sids.contains(&end.sid) {
        return Ok(discard_result());
    }
    let matching_live_binding = validate_session_end_binding(runtime, &end)?;
    validate_session_end_route(runtime, &end)?;
    apply_session_end_state(runtime, &end, matching_live_binding)?;
    let peer_label = update_ended_peer_route(runtime, &end);
    Ok(project_session_end(end, peer_label))
}

fn apply_session_end_state(
    runtime: &mut HydraProfileRuntime,
    end: &KktpSessionEnd,
    matching_live_binding: bool,
) -> Result<(), String> {
    if matching_live_binding {
        apply_live_session_end(runtime, &end.sender_hydra_id)
    } else {
        remember_retired_kktp_sid(runtime, end.sid.clone());
        Ok(())
    }
}

pub(crate) fn decode_mailbox_envelope(envelope_hex: &str) -> Result<Vec<u8>, String> {
    if !envelope_hex.len().is_multiple_of(2)
        || envelope_hex.len() > MAX_MAILBOX_ENVELOPE_BYTES.saturating_mul(2)
    {
        return Err("mailbox envelope hex length is invalid".into());
    }
    let envelope =
        hex::decode(envelope_hex).map_err(|_| "mailbox envelope is not valid hex".to_string())?;
    if envelope.len() < 4 {
        return Err("mailbox envelope header is invalid".into());
    }
    Ok(envelope)
}

pub(crate) fn allowed_session_map(
    active_sessions: Vec<ActiveSessionRegistration>,
) -> Result<HashMap<String, HashSet<String>>, String> {
    if active_sessions.len() > 4096 {
        return Err("too many active Ghost Talk peer sessions".into());
    }
    let mut allowed = HashMap::<String, HashSet<String>>::new();
    for active in active_sessions {
        decode_fixed_hex::<32>(&active.peer_hydra_id, "active HYDRA peer id")?;
        if active.sid.len() != 32 || !active.sid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(
                "active Ghost Talk session SID must be exactly 32 hexadecimal characters".into(),
            );
        }
        allowed
            .entry(active.peer_hydra_id)
            .or_default()
            .insert(active.sid.to_ascii_lowercase());
    }
    Ok(allowed)
}

pub(crate) fn mailbox_anchor_kind(envelope: &[u8]) -> Result<Option<String>, String> {
    if envelope.starts_with(KKTP_ANCHOR_PREFIX) {
        kktp_anchor_type(envelope)
    } else {
        Ok(None)
    }
}

pub(crate) fn mailbox_carrier_kind(envelope: &[u8], anchor_kind: Option<&str>) -> String {
    anchor_kind
        .map(str::to_owned)
        .unwrap_or_else(|| inferred_mailbox_carrier_kind(envelope))
}

fn inferred_mailbox_carrier_kind(envelope: &[u8]) -> String {
    let known = [
        (KKTP_MESSAGE_PREFIX, "message"),
        (ghost_protocol::GTACK_MAGIC.as_slice(), "delivery_ack"),
        (ghost_protocol::GTCR_MAGIC.as_slice(), "contact_request"),
    ]
    .into_iter()
    .find(|(prefix, _)| envelope.starts_with(prefix))
    .map(|(_, kind)| kind);
    known
        .or_else(|| {
            ghost_protocol::kktp_anchor_type(envelope)
                .ok()
                .flatten()
                .filter(|kind| kind == "response")
                .map(|_| "contact_accept")
        })
        .unwrap_or("other")
        .into()
}

mod call_signal;
pub(crate) use call_signal::handle_call_signal_carrier;

pub(crate) fn mailbox_result_with_request(
    request: HydraIncomingRequestProjection,
) -> HydraMailboxResult {
    HydraMailboxResult {
        incoming_request: Some(request),
        ..discard_result()
    }
}

pub(crate) fn handle_contact_request_carrier(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
) -> Result<HydraMailboxResult, String> {
    let request = ghost_protocol::GhostContactRequest::decode(envelope)?;
    ghost_kaspa::verify_contact_request(&request)?;
    if request.sender.hydra_identity_id == runtime.identity_id {
        return Ok(discard_result());
    }
    validate_contact_request_recipient(&request, local_kaspa_addresses)?;
    let card = decode_contact_card(&request)?;
    let contact = runtime.hydra.preview_contact(&card)?;
    if !request.sender.hydra_identity_id.is_empty()
        && request.sender.hydra_identity_id != contact.handle
    {
        return Err(
            "contact-request HYDRA identity does not match its authenticated contact card".into(),
        );
    }
    if contact.handle == runtime.identity_id {
        return Ok(discard_result());
    }
    crate::debug_log::record(
        "info",
        "handshake",
        "discovery-received-verified",
        format!(
            "profile={} sid={} peer={} destination={}",
            profile_id, request.request_id, contact.handle, request.recipient_kaspa_address
        ),
    );
    Ok(mailbox_result_with_request(incoming_request_projection(
        request,
        envelope,
        contact.handle,
    )))
}

pub(crate) fn incoming_request_projection(
    request: ghost_protocol::GhostContactRequest,
    envelope: &[u8],
    peer_hydra_id: String,
) -> HydraIncomingRequestProjection {
    HydraIncomingRequestProjection {
        request_id: request.request_id,
        peer_address: request.sender.kaspa_address,
        local_address: request.recipient_kaspa_address,
        peer_label: request.sender.display_name,
        peer_hydra_id,
        signed_request_hex: hex::encode(envelope),
        room_invite: request.room_invite.map(|invite| HydraRoomInviteProjection {
            room_id: invite.room_id,
            room_name: invite.room_name,
        }),
        call_id: request
            .call_invite
            .as_ref()
            .map(|invite| invite.call_id.clone()),
        call_action: request.call_invite.map(|invite| invite.action),
    }
}
