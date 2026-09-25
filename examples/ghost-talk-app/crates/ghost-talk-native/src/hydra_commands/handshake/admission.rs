use super::super::{
    handshake_restart_rules::{
        handshake_expected_sender, handshake_local_peer, restart_handshake_sid_allowed,
        AuthenticatedHandshake,
    },
    mailbox_dispatch::{discard_result, frame_control},
    runtime_session_queries::{clear_peer_handshake_state, kktp_sid_is_current},
    runtime_state::HydraProfileRuntime,
    session_state::{install_kktp_binding, kktp_handshake_payload, retire_kktp_binding},
    session_types::{
        HydraControlProjection, HydraMailboxResult, KktpHandshakeControl, KktpRole,
        KktpSessionBinding, KktpSessionState, PeerRoute, PendingInbound, Sha256, BASE64,
    },
    transport_persistence::persist_transport_state,
};
use sha2::Digest;
mod authentication;
pub(crate) use authentication::authenticate_handshake;

pub(crate) fn reset_hydra_peer_crypto(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
) -> Result<(), String> {
    let status = runtime
        .hydra
        .has_contact(peer)?
        .then(|| runtime.hydra.session_status(peer))
        .transpose()?;
    match status.as_deref() {
        Some("active") => runtime.hydra.close_session(peer)?,
        Some("pending") => runtime.hydra.abort_handshake(peer)?,
        _ => {}
    }
    Ok(())
}

pub(crate) fn same_pending_init(
    runtime: &HydraProfileRuntime,
    peer: &str,
    sid: &str,
    digest: &str,
) -> bool {
    runtime
        .pending_inbound
        .get(peer)
        .is_some_and(|pending| pending.sid == sid && pending.init_digest.as_deref() == Some(digest))
}

pub(crate) fn reconcile_matching_pq_init(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    existing: &KktpSessionBinding,
    control: &KktpHandshakeControl,
    init_digest: &str,
) -> Result<bool, String> {
    if existing.role != KktpRole::Responder {
        return Err("KKTP session role changed for an existing SID".into());
    }
    if matches!(
        existing.state,
        KktpSessionState::Active | KktpSessionState::Closed
    ) {
        return Ok(false);
    }
    clear_conflicting_pending_init(runtime, peer, existing, control, init_digest)?;
    Ok(true)
}

fn clear_conflicting_pending_init(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    existing: &KktpSessionBinding,
    control: &KktpHandshakeControl,
    init_digest: &str,
) -> Result<(), String> {
    let same_init = same_pending_init(runtime, peer, &control.sid, init_digest);
    if existing.state != KktpSessionState::Handshake || same_init {
        return Ok(());
    }
    if runtime.hydra.session_status(peer)? == "pending" {
        runtime.hydra.abort_handshake(peer)?;
    }
    runtime.pending_inbound.remove(peer);
    Ok(())
}

pub(crate) fn replace_pq_init_binding(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    control: &KktpHandshakeControl,
) -> Result<(), String> {
    reset_hydra_peer_crypto(runtime, peer)?;
    clear_peer_handshake_state(runtime, peer);
    retire_kktp_binding(runtime, peer);
    install_kktp_binding(
        runtime,
        peer,
        control.sid.clone(),
        KktpRole::Responder,
        KktpSessionState::Handshake,
    )?;
    persist_transport_state(runtime)
}

pub(crate) fn ensure_pq_init_binding(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    control: &KktpHandshakeControl,
    init_digest: &str,
) -> Result<bool, String> {
    if runtime.retired_kktp_sids.contains(&control.sid) {
        return Ok(false);
    }
    if let Some(existing) = runtime.kktp_sessions.get(peer).cloned() {
        if existing.sid == control.sid {
            return reconcile_matching_pq_init(runtime, peer, &existing, control, init_digest);
        }
        replace_pq_init_binding(runtime, peer, control)?;
        return Ok(true);
    }
    replace_pq_init_binding(runtime, peer, control)?;
    Ok(true)
}

pub(crate) fn pq_init_answer(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    sid: &str,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    match runtime.hydra.reply_handshake(payload) {
        Ok(answer) => Ok(Some(answer)),
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "handshake",
                "obsolete-pq-init-discarded",
                format!("sid={sid} peer={peer} error={error}"),
            );
            Ok(None)
        }
    }
}

pub(crate) fn control_mailbox_result(
    destination: String,
    payloads_hex: Vec<String>,
    completes_pending_id: Option<String>,
    established_peer: Option<String>,
) -> HydraMailboxResult {
    HydraMailboxResult {
        control: Some(HydraControlProjection {
            destination,
            payloads_hex,
            completes_pending_id,
        }),
        session_established_peer: established_peer,
        ..discard_result()
    }
}

pub(crate) fn handle_pq_init(
    runtime: &mut HydraProfileRuntime,
    control: &KktpHandshakeControl,
    auth: AuthenticatedHandshake,
) -> Result<HydraMailboxResult, String> {
    if auth.local_role != KktpRole::Responder {
        return Err("KKTP pq_init role mapping is invalid".into());
    }
    let init_digest = hex::encode(Sha256::digest(&auth.payload));
    if !ensure_pq_init_binding(runtime, &auth.peer, control, &init_digest)? {
        return Ok(discard_result());
    }
    let Some(answer) = pq_init_answer(runtime, &auth.peer, &control.sid, &auth.payload)? else {
        return Ok(discard_result());
    };
    if let Some(binding) = runtime.kktp_sessions.get_mut(&auth.peer) {
        binding.state = KktpSessionState::Handshake;
    }
    runtime.pending_inbound.insert(
        auth.peer.clone(),
        PendingInbound {
            sid: control.sid.clone(),
            contact_id: auth.peer.clone(),
            init_digest: Some(init_digest),
        },
    );
    let binding = runtime
        .kktp_sessions
        .get(&auth.peer)
        .cloned()
        .ok_or_else(|| "KKTP responder binding disappeared".to_string())?;
    let response = kktp_handshake_payload(runtime, &binding, "pq_resp", &answer, None)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-response-prepared",
        format!("sid={} peer={} state=Handshake", control.sid, auth.peer),
    );
    Ok(control_mailbox_result(
        auth.route.kaspa_address,
        frame_control(response)?.payloads_hex,
        None,
        None,
    ))
}

mod replay;
pub(crate) use replay::{
    replay_prepared_completion, replay_prepared_recovery, valid_initiator_binding,
};
