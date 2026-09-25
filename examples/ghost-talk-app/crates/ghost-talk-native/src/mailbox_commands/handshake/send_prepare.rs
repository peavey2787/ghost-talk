use super::super::send_state::{
    active_kktp_binding, prepare_active_reaction_send, prepare_active_send,
    prepare_restart_if_needed, validate_send_runtime, PreparedSend,
};
use super::handshake_prepare::transport_state::{
    normalize_send_transport_state, persistent_transport_ready, prepare_for_transport_state,
    TransportPrepareRequest,
};

#[expect(
    clippy::too_many_arguments,
    reason = "explicit authenticated send fields"
)]
pub(crate) fn prepare_send_payloads(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    profile_id: &str,
    identity_id: &str,
    contact_id: &str,
    body: &str,
    reaction: Option<&ghost_protocol::GhostReactionEvent>,
    message_id: &str,
    stego_profile: &str,
    reuse_change: bool,
    allow_handshake: bool,
) -> Result<PreparedSend, String> {
    validate_send_runtime(runtime, identity_id, contact_id)?;
    if let Some(reaction) = reaction {
        return prepare_reaction_payloads(
            runtime,
            contact_id,
            message_id,
            reaction,
            stego_profile,
            reuse_change,
            allow_handshake,
        );
    }
    prepare_text_payloads(
        runtime,
        profile_id,
        contact_id,
        body,
        message_id,
        stego_profile,
        reuse_change,
        allow_handshake,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit authenticated text-send fields"
)]
fn prepare_text_payloads(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    profile_id: &str,
    contact_id: &str,
    body: &str,
    message_id: &str,
    stego_profile: &str,
    reuse_change: bool,
    allow_handshake: bool,
) -> Result<PreparedSend, String> {
    prepare_restart_if_needed(runtime, contact_id, allow_handshake)?;
    let kktp_active = active_kktp_binding(runtime, contact_id);
    let mut hydra_status = runtime.hydra.session_status(contact_id)?;
    if persistent_transport_ready(runtime, contact_id) && hydra_status != "active" {
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
    hydra_status = normalize_send_transport_state(
        runtime,
        contact_id,
        hydra_status,
        kktp_active,
        allow_handshake,
    )?;
    prepare_for_transport_state(
        runtime,
        TransportPrepareRequest {
            profile_id,
            contact_id,
            body,
            message_id,
            stego_profile,
            reuse_change,
            allow_handshake,
            hydra_status: &hydra_status,
        },
    )
}

fn prepare_reaction_payloads(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    reaction: &ghost_protocol::GhostReactionEvent,
    stego_profile: &str,
    reuse_change: bool,
    allow_handshake: bool,
) -> Result<PreparedSend, String> {
    if reuse_change || allow_handshake {
        return Err("Reaction events cannot use handshake or realtime send modes".into());
    }
    prepare_active_reaction_send(runtime, contact_id, message_id, reaction, stego_profile)
}
