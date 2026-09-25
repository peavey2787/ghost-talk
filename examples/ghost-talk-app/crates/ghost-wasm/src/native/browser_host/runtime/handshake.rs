use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraControlProjection, HydraMailboxResult};
use ghost_protocol::{KktpFirstMessage, KktpHandshakeControl, GHOST_KKTP_VERSION};
use sha2::{Digest, Sha256};

use super::{secure_transport::with_hydra_runtime, mailbox_common::{discard, frame}, session::{self, Binding, Role, SessionState}};

pub(in crate::native::browser_host) struct Authenticated {
    pub(in crate::native::browser_host) peer: String,
    pub(in crate::native::browser_host) role: Role,
    pub(in crate::native::browser_host) destination: String,
    pub(in crate::native::browser_host) payload: Vec<u8>,
}

pub(in crate::native::browser_host) fn successor_sid(prior: &str, initiator: &str, responder: &str) -> Result<String, String> {
    let prior = fixed_hex::<16>(prior, "persisted KKTP SID")?;
    let initiator = fixed_hex::<32>(initiator, "KKTP initiator HYDRA id")?;
    let responder = fixed_hex::<32>(responder, "KKTP responder HYDRA id")?;
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/KKTP/restart-successor/v1\0");
    hasher.update(prior); hasher.update(initiator); hasher.update(responder);
    Ok(hex::encode(&hasher.finalize()[..16]))
}

fn fixed_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) { return Err(format!("{label} has invalid hexadecimal length")); }
    hex::decode(value).map_err(|_| format!("{label} is not valid hex"))?.try_into().map_err(|_| format!("{label} has the wrong size"))
}

pub(in crate::native::browser_host) fn control(profile: &str, binding: &Binding, stage: &str, payload: &[u8], first: Option<KktpFirstMessage>) -> Result<Vec<u8>, String> {
    let local = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    let (initiator, responder) = match binding.role { Role::Initiator => (local, binding.peer.clone()), Role::Responder => (binding.peer.clone(), local) };
    let mut value = KktpHandshakeControl {
        kind: "ghost_handshake".into(), version: GHOST_KKTP_VERSION, sid: binding.sid.clone(), stage: stage.into(),
        initiator_hydra_id: initiator, responder_hydra_id: responder, payload_b64: BASE64.encode(payload), first_message: first, pq_sig_b64: String::new(),
    };
    let signature = with_hydra_runtime(profile, |hydra| hydra.sign_application_context(&value.signing_bytes()?))?;
    value.pq_sig_b64 = BASE64.encode(signature);
    value.encode()
}

pub(in crate::native::browser_host) fn authenticate(profile: &str, control: &KktpHandshakeControl) -> Result<Option<Authenticated>, String> {
    let local = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    let (peer, role) = handshake_peer(&local, control)?;
    let expected = expected_sender(control)?;
    if expected == local { return Ok(None); }
    if expected != peer { return Err("KKTP PQ handshake sender role is inconsistent".into()); }
    let (destination, allowed, blocked) = route_state(profile, &peer, control, role)?;
    if blocked || !allowed { return Ok(None); }
    verify_handshake_sender(profile, &peer, control)?;
    let payload = BASE64.decode(&control.payload_b64)
        .map_err(|_| "KKTP PQ handshake payload is not valid base64".to_string())?;
    Ok(Some(Authenticated { peer, role, destination, payload }))
}

fn handshake_peer(local: &str, control: &KktpHandshakeControl) -> Result<(String, Role), String> {
    if control.initiator_hydra_id == local {
        Ok((control.responder_hydra_id.clone(), Role::Initiator))
    } else if control.responder_hydra_id == local {
        Ok((control.initiator_hydra_id.clone(), Role::Responder))
    } else {
        Err("KKTP PQ handshake is addressed to another HYDRA identity".into())
    }
}

fn expected_sender(control: &KktpHandshakeControl) -> Result<String, String> {
    match control.stage.as_str() {
        "pq_init" | "pq_finish" => Ok(control.initiator_hydra_id.clone()),
        "pq_resp" => Ok(control.responder_hydra_id.clone()),
        _ => Err("unknown Ghost Talk KKTP PQ handshake stage".into()),
    }
}

fn route_state(
    profile: &str,
    peer: &str,
    control: &KktpHandshakeControl,
    role: Role,
) -> Result<(String, bool, bool), String> {
    session::with(profile, |runtime| {
        let route = runtime.routes.get(peer)
            .ok_or_else(|| "KKTP PQ handshake has no verified Kaspa peer route".to_string())?;
        let current = runtime.sessions.get(peer).is_some_and(|binding| binding.sid == control.sid);
        let allowed = current || restart_allowed(route, control, role);
        Ok((route.kaspa_address.clone(), allowed, runtime.blocked.contains(peer)))
    })
}

fn verify_handshake_sender(profile: &str, peer: &str, control: &KktpHandshakeControl) -> Result<(), String> {
    if !with_hydra_runtime(profile, |hydra| hydra.has_contact(peer))? {
        return Err("KKTP PQ handshake names an unknown HYDRA contact".into());
    }
    let signature = BASE64.decode(&control.pq_sig_b64)
        .map_err(|_| "KKTP PQ context signature is not valid base64".to_string())?;
    with_hydra_runtime(profile, |hydra| {
        hydra.verify_contact_application_context(peer, &control.signing_bytes()?, &signature)
    })
}

fn restart_allowed(route: &session::Route, control: &KktpHandshakeControl, role: Role) -> bool {
    if control.stage != "pq_init" || role != Role::Responder || !route.resume_required || route.session_role != Some(Role::Responder) { return false; }
    route.session_sid.as_deref().is_some_and(|prior| successor_sid(prior, &control.initiator_hydra_id, &control.responder_hydra_id).is_ok_and(|sid| sid == control.sid))
}

pub(in crate::native::browser_host) fn handle(profile: &str, control: KktpHandshakeControl) -> Result<HydraMailboxResult, String> {
    let Some(auth) = authenticate(profile, &control)? else { return Ok(discard()); };
    match control.stage.as_str() {
        "pq_init" => handle_init(profile, &control, auth),
        "pq_resp" => handle_response(profile, &control, auth),
        "pq_finish" => handle_finish(profile, &control, auth),
        _ => Err("unknown Ghost Talk KKTP PQ handshake stage".into()),
    }
}

fn handle_init(profile: &str, control: &KktpHandshakeControl, auth: Authenticated) -> Result<HydraMailboxResult, String> {
    if auth.role != Role::Responder { return Err("KKTP pq_init role mapping is invalid".into()); }
    reset_peer_crypto(profile, &auth.peer)?;
    let binding = session::install(profile, &auth.peer, control.sid.clone(), Role::Responder, SessionState::Handshake)?;
    let answer = with_hydra_runtime(profile, |hydra| hydra.reply_handshake(&auth.payload))?;
    let carrier = control_for_frames(profile, &binding, "pq_resp", &answer, None)?;
    Ok(HydraMailboxResult { control: Some(HydraControlProjection { destination: auth.destination, payloads_hex: carrier, completes_pending_id: None }), ..discard() })
}

fn handle_response(profile: &str, control_value: &KktpHandshakeControl, auth: Authenticated) -> Result<HydraMailboxResult, String> {
    if auth.role != Role::Initiator { return Err("KKTP pq_resp role mapping is invalid".into()); }
    let binding = session::with(profile, |runtime| runtime.sessions.get(&auth.peer).cloned().ok_or_else(|| "KKTP initiator binding disappeared".to_string()))?;
    if binding.sid != control_value.sid { return Ok(discard()); }
    let finish = with_hydra_runtime(profile, |hydra| hydra.finish_handshake(&auth.payload))?;
    session::with_mut(profile, |runtime| { if let Some(binding) = runtime.sessions.get_mut(&auth.peer) { binding.state = SessionState::Active; } Ok(()) })?;
    let pending = session::with(profile, |runtime| Ok(runtime.pending_outbound.get(&auth.peer).cloned()))?;
    let active = session::with(profile, |runtime| runtime.sessions.get(&auth.peer).cloned().ok_or_else(|| "KKTP initiator binding disappeared".to_string()))?;
    if let Some(pending) = pending {
        let first = super::mailbox_send::first_message(profile, &auth.peer, &pending.message_id, &pending.body, &pending.stego_profile)?;
        let payloads = control_for_frames(profile, &active, "pq_finish", &finish, Some(first))?;
        session::with_mut(profile, |runtime| {
            runtime.prepared_completion.insert(auth.peer.clone(), session::PreparedCompletion {
                destination: pending.destination.clone(),
                message_id: pending.message_id.clone(),
                payloads_hex: payloads.clone(),
            });
            Ok(())
        })?;
        return Ok(HydraMailboxResult { control: Some(HydraControlProjection { destination: pending.destination, payloads_hex: payloads, completes_pending_id: Some(pending.id) }), ..discard() });
    }
    let payloads = control_for_frames(profile, &active, "pq_finish", &finish, None)?;
    Ok(HydraMailboxResult { control: Some(HydraControlProjection { destination: auth.destination, payloads_hex: payloads, completes_pending_id: None }), session_established_peer: Some(auth.peer), ..discard() })
}

fn handle_finish(profile: &str, control_value: &KktpHandshakeControl, auth: Authenticated) -> Result<HydraMailboxResult, String> {
    if auth.role != Role::Responder { return Err("KKTP pq_finish role mapping is invalid".into()); }
    with_hydra_runtime(profile, |hydra| hydra.accept_finish(&auth.payload))?;
    session::with_mut(profile, |runtime| { if let Some(binding) = runtime.sessions.get_mut(&auth.peer) { binding.state = SessionState::Active; } Ok(()) })?;
    let (received, message_id) = match control_value.first_message.clone() {
        None => (None, None),
        Some(first) => (super::mailbox_receive::open_first(profile, &auth.peer, &control_value.sid, first.clone())?, Some(first.message_id)),
    };
    Ok(HydraMailboxResult { received, peer_address: Some(auth.destination), message_id, session_established_peer: Some(auth.peer), ..discard() })
}

fn control_for_frames(profile: &str, binding: &Binding, stage: &str, payload: &[u8], first: Option<KktpFirstMessage>) -> Result<Vec<String>, String> {
    Ok(frame(&control(profile, binding, stage, payload, first)?)?.into_iter().map(hex::encode).collect())
}

pub(in crate::native::browser_host) fn reset_peer_crypto(profile: &str, peer: &str) -> Result<(), String> {
    with_hydra_runtime(profile, |hydra| { match hydra.session_status(peer)?.as_str() { "active" => hydra.close_session(peer)?, "pending" => hydra.abort_handshake(peer)?, _ => {} } Ok(()) })
}
