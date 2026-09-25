use super::super::{
    runtime_state::HydraProfileRuntime,
    session_types::{
        make_kktp_binding, Aes256Gcm, KktpDirection, KktpFirstMessage, KktpHandshakeControl,
        KktpRole, KktpSessionBinding, KktpSessionEnd, KktpSessionState, Nonce, OsRng, PeerRoute,
        BASE64, GHOST_KKTP_VERSION, MAX_RETIRED_KKTP_SIDS,
    },
};
use aes_gcm::aead::Payload;
use aes_gcm::{aead::Aead, KeyInit};
use base64::Engine as _;
use rand::RngCore;
pub(crate) fn install_kktp_binding(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    sid: String,
    role: KktpRole,
    state: KktpSessionState,
) -> Result<(), String> {
    let binding = make_kktp_binding(&runtime.identity_id, peer_hydra_id, sid, role, state)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "binding-installed",
        format!(
            "peer={} sid={} role={:?} state={:?} mailbox={}",
            peer_hydra_id, binding.sid, binding.role, binding.state, binding.mailbox_id
        ),
    );
    runtime
        .kktp_sessions
        .insert(peer_hydra_id.to_owned(), binding);
    Ok(())
}

pub(crate) fn remember_retired_kktp_sid(runtime: &mut HydraProfileRuntime, sid: String) {
    // Persist a bounded retired-SID replay set alongside active restart state.
    // Durable mailbox packet IDs remain the primary duplicate boundary, while
    // this prevents a recently retired session from being resurrected after a
    // process restart.
    if runtime.retired_kktp_sids.len() >= MAX_RETIRED_KKTP_SIDS {
        runtime.retired_kktp_sids.clear();
    }
    runtime.retired_kktp_sids.insert(sid);
}

pub(crate) fn mark_peer_route_session_active(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
) {
    let Some(binding) = runtime.kktp_sessions.get(peer_hydra_id).cloned() else {
        return;
    };
    if binding.state != KktpSessionState::Active {
        return;
    }
    if let Some(route) = runtime.peer_routes.get_mut(peer_hydra_id) {
        route.session_sid = Some(binding.sid);
        route.session_role = Some(binding.role);
        route.resume_required = false;
    }
}

pub(crate) fn retire_kktp_binding(runtime: &mut HydraProfileRuntime, peer_hydra_id: &str) {
    if let Some(binding) = runtime.kktp_sessions.remove(peer_hydra_id) {
        crate::debug_log::record(
            "info",
            "handshake",
            "binding-retired",
            format!(
                "peer={} sid={} state={:?}",
                peer_hydra_id, binding.sid, binding.state
            ),
        );
        remember_retired_kktp_sid(runtime, binding.sid);
    }
}

pub(crate) fn session_end_route<'a>(
    runtime: &'a HydraProfileRuntime,
    peer_hydra_id: &str,
    recipient_kaspa_address: &str,
) -> Result<&'a PeerRoute, String> {
    if runtime.blocked_peers.contains(peer_hydra_id) {
        return Err("this peer is already closed for the current chat session".into());
    }
    let route = runtime
        .peer_routes
        .get(peer_hydra_id)
        .ok_or_else(|| "KKTP session has no verified Kaspa peer route".to_string())?;
    if route.kaspa_address != recipient_kaspa_address {
        return Err("session_end destination does not match the authenticated peer route".into());
    }
    Ok(route)
}

pub(crate) fn session_end_sid_role(
    runtime: &HydraProfileRuntime,
    peer_hydra_id: &str,
    route: &PeerRoute,
) -> Result<(String, KktpRole), String> {
    if let Some(binding) = runtime.kktp_sessions.get(peer_hydra_id) {
        if binding.state != KktpSessionState::Active {
            return Err(
                "the KKTP session is not active, so no session_end anchor can be emitted".into(),
            );
        }
        return Ok((binding.sid.clone(), binding.role));
    }
    let sid = route
        .session_sid
        .clone()
        .ok_or_else(|| "there is no persisted KKTP SID to end for this peer".to_string())?;
    let role = route.session_role.ok_or_else(|| {
        "the persisted KKTP role is unavailable for this pre-KKTP session; establish a fresh secure session before emitting session_end".to_string()
    })?;
    Ok((sid, role))
}

pub(crate) fn session_end_participants(
    local_hydra_id: &str,
    peer_hydra_id: &str,
    role: KktpRole,
) -> (String, String) {
    match role {
        KktpRole::Initiator => (local_hydra_id.to_owned(), peer_hydra_id.to_owned()),
        KktpRole::Responder => (peer_hydra_id.to_owned(), local_hydra_id.to_owned()),
    }
}

pub(crate) fn sign_session_end(
    runtime: &HydraProfileRuntime,
    mut end: KktpSessionEnd,
) -> Result<KktpSessionEnd, String> {
    let signature = runtime
        .hydra
        .sign_application_context(&end.pq_signing_bytes()?)?;
    end.pq_sig_b64 = BASE64.encode(signature);
    Ok(end)
}

pub(crate) fn prepare_kktp_session_end(
    runtime: &HydraProfileRuntime,
    peer_hydra_id: &str,
    sender_kaspa_address: &str,
    recipient_kaspa_address: &str,
    expected_session_sid: Option<&str>,
    reason: &str,
) -> Result<KktpSessionEnd, String> {
    if let Some(expected_sid) = expected_session_sid {
        let current_sid = runtime
            .kktp_sessions
            .get(peer_hydra_id)
            .map(|binding| binding.sid.as_str())
            .or_else(|| {
                runtime
                    .peer_routes
                    .get(peer_hydra_id)
                    .and_then(|route| route.session_sid())
            });
        if current_sid != Some(expected_sid) {
            return Err("stale chat leave no longer matches the active KKTP session".into());
        }
    }
    let route = session_end_route(runtime, peer_hydra_id, recipient_kaspa_address)?;
    let (sid, role) = session_end_sid_role(runtime, peer_hydra_id, route)?;
    let (initiator_hydra_id, responder_hydra_id) =
        session_end_participants(&runtime.identity_id, peer_hydra_id, role);
    sign_session_end(
        runtime,
        KktpSessionEnd {
            kind: "session_end".into(),
            version: GHOST_KKTP_VERSION,
            sid,
            initiator_hydra_id,
            responder_hydra_id,
            sender_hydra_id: runtime.identity_id.clone(),
            sender_kaspa_address: sender_kaspa_address.to_owned(),
            recipient_kaspa_address: recipient_kaspa_address.to_owned(),
            reason: reason.to_owned(),
            pq_sig_b64: String::new(),
            sig: String::new(),
        },
    )
}

pub(crate) fn kktp_handshake_payload(
    runtime: &HydraProfileRuntime,
    binding: &KktpSessionBinding,
    stage: &str,
    payload: &[u8],
    first_message: Option<KktpFirstMessage>,
) -> Result<Vec<u8>, String> {
    let (initiator_hydra_id, responder_hydra_id) = match binding.role {
        KktpRole::Initiator => (runtime.identity_id.clone(), binding.peer_hydra_id.clone()),
        KktpRole::Responder => (binding.peer_hydra_id.clone(), runtime.identity_id.clone()),
    };
    let mut control = KktpHandshakeControl {
        kind: "ghost_handshake".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid.clone(),
        stage: stage.into(),
        initiator_hydra_id,
        responder_hydra_id,
        payload_b64: BASE64.encode(payload),
        first_message,
        pq_sig_b64: String::new(),
    };
    let signature = runtime
        .hydra
        .sign_application_context(&control.signing_bytes()?)?;
    control.pq_sig_b64 = BASE64.encode(signature);
    control.encode()
}

pub(crate) fn persistent_durable_aad(
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    seq: u64,
    message_id: &str,
) -> Vec<u8> {
    format!(
        "GhostTalk/PersistentTransport/durable/v1\0{sid}\0{mailbox_id}\0{}\0{seq}\0{message_id}",
        direction.as_str()
    )
    .into_bytes()
}

pub(crate) fn persistent_realtime_aad(
    sid: &str,
    sender_hydra_id: &str,
    message_id: &str,
) -> Vec<u8> {
    format!("GhostTalk/PersistentTransport/realtime/v1\0{sid}\0{sender_hydra_id}\0{message_id}")
        .into_bytes()
}

pub(crate) fn seal_persistent_payload(
    key: &[u8; 32],
    magic: &[u8; 4],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "persistent transport key has invalid length".to_string())?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| "persistent transport encryption failed".to_string())?;
    let mut out = Vec::with_capacity(magic.len() + nonce.len() + ciphertext.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub(crate) fn open_persistent_payload(
    key: &[u8; 32],
    magic: &[u8; 4],
    aad: &[u8],
    encrypted: &[u8],
) -> Result<Vec<u8>, String> {
    if encrypted.len() <= magic.len() + 12 || !encrypted.starts_with(magic) {
        return Err("persistent transport envelope is malformed".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "persistent transport key has invalid length".to_string())?;
    cipher
        .decrypt(
            Nonce::from_slice(&encrypted[magic.len()..magic.len() + 12]),
            Payload {
                msg: &encrypted[magic.len() + 12..],
                aad,
            },
        )
        .map_err(|_| "persistent transport authentication failed".to_string())
}
