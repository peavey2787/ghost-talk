use super::{
    prepare_active_send, prepare_new_handshake, prepare_pending_handshake,
    responder_handshake_waiting, PreparedSend,
};

pub(in crate::mailbox_commands::handshake) fn persistent_transport_ready(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
) -> bool {
    runtime
        .kktp_sessions
        .get(contact_id)
        .is_some_and(|binding| {
            binding.state == crate::hydra_commands::KktpSessionState::Active
                && binding.persistent_transport.is_some()
        })
}

pub(in crate::mailbox_commands::handshake) fn normalize_send_transport_state(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    mut hydra_status: String,
    kktp_active: bool,
    allow_handshake: bool,
) -> Result<String, String> {
    // A fresh authenticated binding can replace a previous logical chat while
    // HYDRA still reports its old volatile session as active. Align the crypto
    // session with the current KKTP binding before sending anything new.
    if hydra_status == "active" && !kktp_active {
        hydra_status = reset_unbound_active_session(runtime, contact_id, allow_handshake)?;
    }
    if should_reset_unowned_pending(runtime, contact_id, &hydra_status) {
        hydra_status = reset_unowned_pending_session(runtime, contact_id, allow_handshake)?;
    }
    Ok(hydra_status)
}

fn reset_unbound_active_session(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    allow_handshake: bool,
) -> Result<String, String> {
    if responder_handshake_waiting(runtime, contact_id) {
        return Err("Waiting for the chat initiator's authenticated PQ handshake".into());
    }
    if !allow_handshake {
        return Err("Secure peer transport is not active; no new handshake was started".into());
    }
    runtime.hydra.close_session(contact_id)?;
    runtime.hydra.session_status(contact_id)
}

fn should_reset_unowned_pending(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    hydra_status: &str,
) -> bool {
    hydra_status == "pending"
        && !runtime.pending_outbound.contains_key(contact_id)
        && !responder_handshake_waiting(runtime, contact_id)
}

fn reset_unowned_pending_session(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    allow_handshake: bool,
) -> Result<String, String> {
    if !allow_handshake {
        return Err(
            "Secure peer transport is still establishing; no new handshake was started".into(),
        );
    }
    runtime.hydra.abort_handshake(contact_id)?;
    runtime.hydra.session_status(contact_id)
}

pub(in crate::mailbox_commands::handshake) struct TransportPrepareRequest<'a> {
    pub(in crate::mailbox_commands::handshake) profile_id: &'a str,
    pub(in crate::mailbox_commands::handshake) contact_id: &'a str,
    pub(in crate::mailbox_commands::handshake) body: &'a str,
    pub(in crate::mailbox_commands::handshake) message_id: &'a str,
    pub(in crate::mailbox_commands::handshake) stego_profile: &'a str,
    pub(in crate::mailbox_commands::handshake) reuse_change: bool,
    pub(in crate::mailbox_commands::handshake) allow_handshake: bool,
    pub(in crate::mailbox_commands::handshake) hydra_status: &'a str,
}

pub(in crate::mailbox_commands::handshake) fn prepare_for_transport_state(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    request: TransportPrepareRequest<'_>,
) -> Result<PreparedSend, String> {
    let TransportPrepareRequest {
        profile_id,
        contact_id,
        body,
        message_id,
        stego_profile,
        reuse_change,
        allow_handshake,
        hydra_status,
    } = request;
    if hydra_status == "active" {
        return prepare_active_send(
            runtime,
            contact_id,
            message_id,
            body,
            stego_profile,
            reuse_change,
            allow_handshake,
        );
    }
    if matches!(hydra_status, "missing" | "closed") {
        return prepare_new_handshake(
            runtime,
            profile_id,
            contact_id,
            body,
            message_id,
            stego_profile,
            allow_handshake,
        );
    }
    if hydra_status == "pending" {
        return prepare_pending_handshake(
            runtime,
            contact_id,
            body,
            message_id,
            stego_profile,
            allow_handshake,
        );
    }
    Err("HYDRA returned an unknown session state".into())
}
