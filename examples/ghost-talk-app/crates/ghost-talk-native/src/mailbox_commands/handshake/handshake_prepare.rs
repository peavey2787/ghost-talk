use super::super::send_state::{
    authenticated_destination, initiator_sid, prepare_active_send, responder_handshake_waiting,
    PreparedSend,
};

pub(crate) fn prepare_new_handshake(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    profile_id: &str,
    contact_id: &str,
    body: &str,
    message_id: &str,
    stego_profile: &str,
    allow_handshake: bool,
) -> Result<PreparedSend, String> {
    validate_new_handshake(runtime, contact_id, allow_handshake)?;
    let sid = initiator_sid(runtime, contact_id);
    let destination = authenticated_destination(
        runtime,
        contact_id,
        "secure-session handshake has no authenticated Kaspa peer route",
    )?;
    let payloads_hex = initialize_handshake(runtime, contact_id, &sid)?;
    let pending_id = remember_pending_outbound(
        runtime,
        contact_id,
        &sid,
        &destination,
        body,
        message_id,
        stego_profile,
        &payloads_hex,
    );
    runtime.prepared_completion.remove(contact_id);
    log_handshake_prepared(profile_id, &sid, contact_id, &pending_id, message_id);
    Ok(PreparedSend {
        payloads: decode_mailbox_payloads(&payloads_hex)?,
        pending_id: Some(pending_id),
        destination,
    })
}

fn validate_new_handshake(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    allow_handshake: bool,
) -> Result<(), String> {
    if !allow_handshake {
        return Err("Secure peer transport is not active; no new handshake was started".into());
    }
    if !runtime.hydra.has_contact(contact_id)? {
        return Err("The recipient has not accepted this Ghost Talk chat yet".into());
    }
    if responder_handshake_waiting(runtime, contact_id) {
        return Err("Waiting for the chat initiator's authenticated PQ handshake".into());
    }
    Ok(())
}

fn initialize_handshake(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    sid: &str,
) -> Result<Vec<String>, String> {
    reset_hydra_session(runtime, contact_id)?;
    crate::hydra_commands::install_kktp_binding(
        runtime,
        contact_id,
        sid.to_owned(),
        crate::hydra_commands::KktpRole::Initiator,
        crate::hydra_commands::KktpSessionState::Handshake,
    )?;
    let offer = runtime.hydra.init_handshake(contact_id)?;
    let binding = runtime
        .kktp_sessions
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "KKTP initiator binding disappeared".to_string())?;
    let control =
        crate::hydra_commands::kktp_handshake_payload(runtime, &binding, "pq_init", &offer, None)?;
    Ok(crate::hydra_commands::frame_control(control)?.payloads_hex)
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit durable handshake checkpoint fields"
)]
fn remember_pending_outbound(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    sid: &str,
    destination: &str,
    body: &str,
    message_id: &str,
    stego_profile: &str,
    payloads_hex: &[String],
) -> String {
    let pending_id = ghost_core::Id128::new_random().to_string();
    runtime.pending_outbound.insert(
        contact_id.to_owned(),
        crate::hydra_commands::PendingOutbound {
            id: pending_id.clone(),
            sid: sid.to_owned(),
            contact_id: contact_id.to_owned(),
            destination: destination.to_owned(),
            body: body.to_owned(),
            message_id: message_id.to_owned(),
            stego_profile: stego_profile.to_owned(),
            offer_payloads_hex: payloads_hex.to_vec(),
        },
    );
    pending_id
}

fn log_handshake_prepared(
    profile_id: &str,
    sid: &str,
    contact_id: &str,
    pending_id: &str,
    message_id: &str,
) {
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-init-prepared",
        format!(
            "profile={} sid={} peer={} pending={} message={}",
            profile_id, sid, contact_id, pending_id, message_id
        ),
    );
}

fn reset_hydra_session(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
) -> Result<(), String> {
    match runtime.hydra.session_status(contact_id)?.as_str() {
        "active" => runtime.hydra.close_session(contact_id),
        "pending" => runtime.hydra.abort_handshake(contact_id),
        _ => Ok(()),
    }
}

pub(crate) fn validate_pending_send(
    pending: &crate::hydra_commands::PendingOutbound,
    contact_id: &str,
    body: &str,
    message_id: &str,
    stego_profile: &str,
) -> Result<(), String> {
    let matches = pending.contact_id == contact_id
        && pending.body == body
        && pending.message_id == message_id
        && pending.stego_profile == stego_profile;
    if matches {
        Ok(())
    } else {
        Err("The secure session is still being established for a previous message".into())
    }
}

pub(crate) fn prepare_pending_handshake(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    body: &str,
    message_id: &str,
    stego_profile: &str,
    allow_handshake: bool,
) -> Result<PreparedSend, String> {
    if !allow_handshake {
        return Err(
            "Secure peer transport is still establishing; no new handshake was started".into(),
        );
    }
    let pending = runtime
        .pending_outbound
        .get_mut(contact_id)
        .ok_or_else(|| {
            "A secure-session handshake is already pending for this peer; retry after it completes"
                .to_string()
        })?;

    if validate_pending_send(pending, contact_id, body, message_id, stego_profile).is_err()
        && !adopt_restore_message(pending, body, message_id, stego_profile)
    {
        validate_pending_send(pending, contact_id, body, message_id, stego_profile)?;
    }

    Ok(PreparedSend {
        payloads: decode_mailbox_payloads(&pending.offer_payloads_hex)?,
        pending_id: Some(pending.id.clone()),
        destination: pending.destination.clone(),
    })
}

fn adopt_restore_message(
    pending: &mut crate::hydra_commands::PendingOutbound,
    body: &str,
    message_id: &str,
    stego_profile: &str,
) -> bool {
    if pending.body != ghost_core::SESSION_RESTORE_BODY {
        return false;
    }
    pending.body = body.to_owned();
    pending.message_id = message_id.to_owned();
    pending.stego_profile = stego_profile.to_owned();
    crate::debug_log::record(
        "info",
        "handshake",
        "restart-restore-adopted-user-message",
        format!(
            "sid={} peer={} pending={} message={}",
            pending.sid, pending.contact_id, pending.id, pending.message_id
        ),
    );
    true
}

pub(super) mod transport_state;
use super::super::gateway::decode_mailbox_payloads;
