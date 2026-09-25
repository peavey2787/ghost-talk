use super::{
    discard_result, mark_peer_route_session_active, open_kktp_envelope, AuthenticatedHandshake,
    HydraMailboxResult, HydraProfileRuntime, KktpFirstMessage, KktpHandshakeControl, KktpRole,
    KktpSessionBinding, KktpSessionState, PeerRoute, ReceivedProjection, BASE64,
};
use base64::Engine as _;

pub(crate) fn replayed_finish_result(
    binding: &KktpSessionBinding,
    control: &KktpHandshakeControl,
    route: &PeerRoute,
    peer: &str,
) -> HydraMailboxResult {
    let Some(first) = control.first_message.as_ref() else {
        return discard_result();
    };
    if binding.state != KktpSessionState::Active {
        return discard_result();
    }
    HydraMailboxResult {
        peer_address: Some(route.kaspa_address.clone()),
        peer_label: Some(route.display_name.clone()),
        message_id: Some(first.message_id.clone()),
        session_established_peer: Some(peer.to_owned()),
        ..discard_result()
    }
}

pub(crate) fn accept_pq_finish(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    sid: &str,
    payload: &[u8],
) -> Result<bool, String> {
    match runtime.hydra.accept_finish(payload) {
        Ok(()) => Ok(true),
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "handshake",
                "obsolete-pq-finish-discarded",
                format!("sid={sid} peer={peer} error={error}"),
            );
            Ok(false)
        }
    }
}

pub(crate) fn open_first_handshake_message(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    sid: &str,
    first: Option<KktpFirstMessage>,
) -> Result<(Option<ReceivedProjection>, Option<String>), String> {
    let Some(first) = first else {
        return Ok((None, None));
    };
    let encrypted = BASE64
        .decode(&first.envelope_b64)
        .map_err(|_| "KKTP first-message envelope is not valid base64".to_string())?;
    let received = open_kktp_envelope(
        runtime,
        peer,
        sid,
        &first.mailbox_id,
        first.direction,
        first.seq,
        &first.message_id,
        &first.profile,
        &encrypted,
    )?;
    Ok((received, Some(first.message_id)))
}

pub(crate) fn established_handshake_result(
    received: Option<ReceivedProjection>,
    route: PeerRoute,
    message_id: Option<String>,
    peer: String,
) -> HydraMailboxResult {
    HydraMailboxResult {
        received,
        peer_address: Some(route.kaspa_address),
        peer_label: Some(route.display_name),
        message_id,
        session_established_peer: Some(peer),
        ..discard_result()
    }
}

fn finish_binding(
    runtime: &HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: &AuthenticatedHandshake,
) -> Result<Option<KktpSessionBinding>, String> {
    if auth.local_role != KktpRole::Responder {
        return Err("KKTP pq_finish role mapping is invalid".into());
    }
    let Some(binding) = runtime.kktp_sessions.get(&auth.peer).cloned() else {
        crate::debug_log::record(
            "warn",
            "handshake",
            "orphan-pq-finish-discarded",
            format!("sid={} peer={}", control.sid, auth.peer),
        );
        return Ok(None);
    };
    Ok((binding.sid == control.sid && binding.role == KktpRole::Responder).then_some(binding))
}

fn activate_finish(
    runtime: &mut HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: &AuthenticatedHandshake,
) -> Result<bool, String> {
    let Some(pending) = runtime.pending_inbound.get(&auth.peer).cloned() else {
        return Ok(false);
    };
    if pending.contact_id != auth.peer || pending.sid != control.sid {
        return Ok(false);
    }
    if !accept_pq_finish(runtime, &auth.peer, &control.sid, &auth.payload)? {
        return Ok(false);
    }
    if let Some(current) = runtime.kktp_sessions.get_mut(&auth.peer) {
        current.state = KktpSessionState::Active;
    }
    mark_peer_route_session_active(runtime, &auth.peer);
    runtime.pending_inbound.remove(&auth.peer);
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-finish-accepted-session-active",
        format!(
            "sid={} peer={} first_message={}",
            control.sid,
            auth.peer,
            control.first_message.is_some()
        ),
    );
    Ok(true)
}

pub(crate) fn handle_pq_finish(
    runtime: &mut HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: AuthenticatedHandshake,
) -> Result<HydraMailboxResult, String> {
    let Some(binding) = finish_binding(runtime, control, &auth)? else {
        return Ok(discard_result());
    };
    if !runtime.pending_inbound.contains_key(&auth.peer) {
        return Ok(replayed_finish_result(
            &binding,
            control,
            &auth.route,
            &auth.peer,
        ));
    }
    if !activate_finish(runtime, control, &auth)? {
        return Ok(discard_result());
    }
    let (received, message_id) = open_first_handshake_message(
        runtime,
        &auth.peer,
        &control.sid,
        control.first_message.clone(),
    )?;
    Ok(established_handshake_result(
        received, auth.route, message_id, auth.peer,
    ))
}
