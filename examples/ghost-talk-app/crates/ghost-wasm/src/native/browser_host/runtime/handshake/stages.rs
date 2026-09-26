//! KKTP handshake stage transitions (pq_init, pq_resp, pq_finish).

use ghost_api::{HydraControlProjection, HydraMailboxResult};
use ghost_protocol::{KktpFirstMessage, KktpHandshakeControl};

use super::super::{
    mailbox::{discard, frame},
    secure_transport::with_hydra_runtime,
    session::{self, Binding, Role, SessionState},
};

use super::{authenticate, control, reset_peer_crypto, Authenticated};

pub(in crate::native::browser_host) fn handle(
    profile: &str,
    control: KktpHandshakeControl,
) -> Result<HydraMailboxResult, String> {
    let Some(auth) = authenticate(profile, &control)? else {
        return Ok(discard());
    };
    match control.stage.as_str() {
        "pq_init" => handle_init(profile, &control, auth),
        "pq_resp" => handle_response(profile, &control, auth),
        "pq_finish" => handle_finish(profile, &control, auth),
        _ => Err("unknown Ghost Talk KKTP PQ handshake stage".into()),
    }
}

fn handle_init(
    profile: &str,
    control: &KktpHandshakeControl,
    auth: Authenticated,
) -> Result<HydraMailboxResult, String> {
    if auth.role != Role::Responder {
        return Err("KKTP pq_init role mapping is invalid".into());
    }
    reset_peer_crypto(profile, &auth.peer)?;
    let binding = session::install(
        profile,
        &auth.peer,
        control.sid.clone(),
        Role::Responder,
        SessionState::Handshake,
    )?;
    let answer = with_hydra_runtime(profile, |hydra| hydra.reply_handshake(&auth.payload))?;
    let carrier = control_for_frames(profile, &binding, "pq_resp", &answer, None)?;
    Ok(HydraMailboxResult {
        control: Some(HydraControlProjection {
            destination: auth.destination,
            payloads_hex: carrier,
            completes_pending_id: None,
        }),
        ..discard()
    })
}

fn handle_response(
    profile: &str,
    control_value: &KktpHandshakeControl,
    auth: Authenticated,
) -> Result<HydraMailboxResult, String> {
    if auth.role != Role::Initiator {
        return Err("KKTP pq_resp role mapping is invalid".into());
    }
    if initiator_binding(profile, &auth.peer)?.sid != control_value.sid {
        return Ok(discard());
    }
    let finish = with_hydra_runtime(profile, |hydra| hydra.finish_handshake(&auth.payload))?;
    mark_active(profile, &auth.peer)?;
    let active = initiator_binding(profile, &auth.peer)?;
    let pending = session::with(profile, |runtime| {
        Ok(runtime.pending_outbound.get(&auth.peer).cloned())
    })?;
    if let Some(pending) = pending {
        return finish_with_first_message(profile, &auth.peer, &active, &finish, pending);
    }
    let payloads = control_for_frames(profile, &active, "pq_finish", &finish, None)?;
    Ok(HydraMailboxResult {
        control: Some(HydraControlProjection {
            destination: auth.destination,
            payloads_hex: payloads,
            completes_pending_id: None,
        }),
        session_established_peer: Some(auth.peer),
        ..discard()
    })
}

fn initiator_binding(profile: &str, peer: &str) -> Result<session::Binding, String> {
    session::with(profile, |runtime| {
        runtime
            .sessions
            .get(peer)
            .cloned()
            .ok_or_else(|| "KKTP initiator binding disappeared".to_string())
    })
}

fn mark_active(profile: &str, peer: &str) -> Result<(), String> {
    session::with_mut(profile, |runtime| {
        if let Some(binding) = runtime.sessions.get_mut(peer) {
            binding.state = SessionState::Active;
        }
        Ok(())
    })
}

/// FINISH carries the queued first message; the exact frames are retained so
/// an ambiguous submission can be retried without re-encrypting.
fn finish_with_first_message(
    profile: &str,
    peer: &str,
    active: &session::Binding,
    finish: &[u8],
    pending: session::PendingSend,
) -> Result<HydraMailboxResult, String> {
    let first = super::super::mailbox::first_message(
        profile,
        peer,
        &pending.message_id,
        &pending.body,
        &pending.stego_profile,
    )?;
    let payloads = control_for_frames(profile, active, "pq_finish", finish, Some(first))?;
    session::with_mut(profile, |runtime| {
        runtime.prepared_completion.insert(
            peer.to_owned(),
            session::PreparedCompletion {
                destination: pending.destination.clone(),
                message_id: pending.message_id.clone(),
                payloads_hex: payloads.clone(),
            },
        );
        Ok(())
    })?;
    Ok(HydraMailboxResult {
        control: Some(HydraControlProjection {
            destination: pending.destination,
            payloads_hex: payloads,
            completes_pending_id: Some(pending.id),
        }),
        ..discard()
    })
}

fn handle_finish(
    profile: &str,
    control_value: &KktpHandshakeControl,
    auth: Authenticated,
) -> Result<HydraMailboxResult, String> {
    if auth.role != Role::Responder {
        return Err("KKTP pq_finish role mapping is invalid".into());
    }
    with_hydra_runtime(profile, |hydra| hydra.accept_finish(&auth.payload))?;
    mark_active(profile, &auth.peer)?;
    let (received, message_id) = match control_value.first_message.clone() {
        None => (None, None),
        Some(first) => (
            super::super::mailbox::open_first(
                profile,
                &auth.peer,
                &control_value.sid,
                first.clone(),
            )?,
            Some(first.message_id),
        ),
    };
    Ok(HydraMailboxResult {
        received,
        peer_address: Some(auth.destination),
        message_id,
        session_established_peer: Some(auth.peer),
        ..discard()
    })
}

fn control_for_frames(
    profile: &str,
    binding: &Binding,
    stage: &str,
    payload: &[u8],
    first: Option<KktpFirstMessage>,
) -> Result<Vec<String>, String> {
    Ok(frame(&control(profile, binding, stage, payload, first)?)?
        .into_iter()
        .map(hex::encode)
        .collect())
}
