use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_protocol::{KktpFirstMessage, KktpHandshakeControl, GHOST_KKTP_VERSION};
use sha2::{Digest, Sha256};

use super::{
    secure_transport::with_hydra_runtime,
    session::{self, Binding, Role},
};

mod stages;

pub(in crate::native::browser_host) use stages::handle;

pub(in crate::native::browser_host) struct Authenticated {
    pub(in crate::native::browser_host) peer: String,
    pub(in crate::native::browser_host) role: Role,
    pub(in crate::native::browser_host) destination: String,
    pub(in crate::native::browser_host) payload: Vec<u8>,
}

pub(in crate::native::browser_host) fn successor_sid(
    prior: &str,
    initiator: &str,
    responder: &str,
) -> Result<String, String> {
    let prior = fixed_hex::<16>(prior, "persisted KKTP SID")?;
    let initiator = fixed_hex::<32>(initiator, "KKTP initiator HYDRA id")?;
    let responder = fixed_hex::<32>(responder, "KKTP responder HYDRA id")?;
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/KKTP/restart-successor/v1\0");
    hasher.update(prior);
    hasher.update(initiator);
    hasher.update(responder);
    Ok(hex::encode(&hasher.finalize()[..16]))
}

fn fixed_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} has invalid hexadecimal length"));
    }
    hex::decode(value)
        .map_err(|_| format!("{label} is not valid hex"))?
        .try_into()
        .map_err(|_| format!("{label} has the wrong size"))
}

pub(in crate::native::browser_host) fn control(
    profile: &str,
    binding: &Binding,
    stage: &str,
    payload: &[u8],
    first: Option<KktpFirstMessage>,
) -> Result<Vec<u8>, String> {
    let local = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    let (initiator, responder) = match binding.role {
        Role::Initiator => (local, binding.peer.clone()),
        Role::Responder => (binding.peer.clone(), local),
    };
    let mut value = KktpHandshakeControl {
        kind: "ghost_handshake".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid.clone(),
        stage: stage.into(),
        initiator_hydra_id: initiator,
        responder_hydra_id: responder,
        payload_b64: BASE64.encode(payload),
        first_message: first,
        pq_sig_b64: String::new(),
    };
    let signature = with_hydra_runtime(profile, |hydra| {
        hydra.sign_application_context(&value.signing_bytes()?)
    })?;
    value.pq_sig_b64 = BASE64.encode(signature);
    value.encode()
}

pub(in crate::native::browser_host) fn authenticate(
    profile: &str,
    control: &KktpHandshakeControl,
) -> Result<Option<Authenticated>, String> {
    let local = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    let (peer, role) = handshake_peer(&local, control)?;
    let expected = expected_sender(control)?;
    if expected == local {
        return Ok(None);
    }
    if expected != peer {
        return Err("KKTP PQ handshake sender role is inconsistent".into());
    }
    let (destination, allowed, blocked) = route_state(profile, &peer, control, role)?;
    if blocked || !allowed {
        return Ok(None);
    }
    verify_handshake_sender(profile, &peer, control)?;
    let payload = BASE64
        .decode(&control.payload_b64)
        .map_err(|_| "KKTP PQ handshake payload is not valid base64".to_string())?;
    Ok(Some(Authenticated {
        peer,
        role,
        destination,
        payload,
    }))
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
        let route = runtime
            .routes
            .get(peer)
            .ok_or_else(|| "KKTP PQ handshake has no verified Kaspa peer route".to_string())?;
        let current = runtime
            .sessions
            .get(peer)
            .is_some_and(|binding| binding.sid == control.sid);
        let allowed = current || restart_allowed(route, control, role);
        Ok((
            route.kaspa_address.clone(),
            allowed,
            runtime.blocked.contains(peer),
        ))
    })
}

fn verify_handshake_sender(
    profile: &str,
    peer: &str,
    control: &KktpHandshakeControl,
) -> Result<(), String> {
    if !with_hydra_runtime(profile, |hydra| hydra.has_contact(peer))? {
        return Err("KKTP PQ handshake names an unknown HYDRA contact".into());
    }
    let signature = BASE64
        .decode(&control.pq_sig_b64)
        .map_err(|_| "KKTP PQ context signature is not valid base64".to_string())?;
    with_hydra_runtime(profile, |hydra| {
        hydra.verify_contact_application_context(peer, &control.signing_bytes()?, &signature)
    })
}

fn restart_allowed(route: &session::Route, control: &KktpHandshakeControl, role: Role) -> bool {
    if control.stage != "pq_init"
        || role != Role::Responder
        || !route.resume_required
        || route.session_role != Some(Role::Responder)
    {
        return false;
    }
    route.session_sid.as_deref().is_some_and(|prior| {
        successor_sid(
            prior,
            &control.initiator_hydra_id,
            &control.responder_hydra_id,
        )
        .is_ok_and(|sid| sid == control.sid)
    })
}

pub(in crate::native::browser_host) fn reset_peer_crypto(
    profile: &str,
    peer: &str,
) -> Result<(), String> {
    with_hydra_runtime(profile, |hydra| {
        match hydra.session_status(peer)?.as_str() {
            "active" => hydra.close_session(peer)?,
            "pending" => hydra.abort_handshake(peer)?,
            _ => {}
        }
        Ok(())
    })
}
