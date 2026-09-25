use super::super::session_types::{
    restart_successor_sid, KktpHandshakeControl, KktpRole, PeerRoute,
};

#[derive(Clone)]
pub(crate) struct AuthenticatedHandshake {
    pub(crate) peer: String,
    pub(crate) local_role: KktpRole,
    pub(crate) route: PeerRoute,
    pub(crate) payload: Vec<u8>,
}

pub(crate) fn handshake_local_peer(
    local: &str,
    control: &KktpHandshakeControl,
) -> Result<(String, KktpRole), String> {
    if control.initiator_hydra_id == local {
        return Ok((control.responder_hydra_id.clone(), KktpRole::Initiator));
    }
    if control.responder_hydra_id == local {
        return Ok((control.initiator_hydra_id.clone(), KktpRole::Responder));
    }
    Err("KKTP PQ handshake is addressed to another HYDRA identity".into())
}

pub(crate) fn handshake_expected_sender(control: &KktpHandshakeControl) -> Result<&str, String> {
    match control.stage.as_str() {
        "pq_init" | "pq_finish" => Ok(&control.initiator_hydra_id),
        "pq_resp" => Ok(&control.responder_hydra_id),
        _ => Err("unknown Ghost Talk KKTP PQ handshake stage".into()),
    }
}

pub(crate) fn restart_handshake_sid_allowed(
    route: &PeerRoute,
    control: &KktpHandshakeControl,
    local_role: KktpRole,
) -> bool {
    let restart_init = control.stage == "pq_init"
        && local_role == KktpRole::Responder
        && route.resume_required
        && route.session_role == Some(KktpRole::Responder);
    if !restart_init {
        return false;
    }
    route.session_sid().is_some_and(|prior_sid| {
        restart_successor_sid(
            prior_sid,
            &control.initiator_hydra_id,
            &control.responder_hydra_id,
        )
        .is_ok_and(|expected| expected == control.sid)
    })
}
