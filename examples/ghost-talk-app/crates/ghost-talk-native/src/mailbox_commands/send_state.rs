use super::gateway::decode_mailbox_payloads;
pub(crate) use ghost_api::{
    DirectoryLiveEvent, MailboxEvent, MailboxSendResult, NetworkStatusEvent, WalletLiveEvent,
};
pub(crate) use ghost_kaspa::{wallet::WalletPublic, LiveTransactionObservation, PortalFacade};
pub(crate) use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
pub(crate) use tauri::{AppHandle, State};

pub(crate) const MAILBOX_SCAN_INTERVAL: Duration = Duration::from_millis(100);

mod validation;
pub(crate) use validation::{validate_message_send, validate_send_runtime};

#[derive(Default)]
pub struct MonitorState {
    pub(crate) generations: Arc<Mutex<HashMap<String, u64>>>,
    /// Latest public HD cursor state for each running profile. The monitor task
    /// must not be restarted just because receive/change indices rotate, but it
    /// also must not keep scanning a stale advertised receive index forever.
    pub(crate) publics: Arc<Mutex<HashMap<String, WalletPublic>>>,
}

pub(crate) struct LiveBlockStream {
    pub(crate) blocks: tokio::sync::broadcast::Receiver<ghost_kaspa::LiveBlockEvent>,
}

pub(crate) struct PreparedSend {
    pub(crate) payloads: Vec<Vec<u8>>,
    pub(crate) pending_id: Option<String>,
    pub(crate) destination: String,
}

pub(crate) fn active_kktp_binding(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
) -> bool {
    runtime
        .kktp_sessions
        .get(contact_id)
        .is_some_and(|binding| binding.state == crate::hydra_commands::KktpSessionState::Active)
}

pub(crate) fn prepare_restart_if_needed(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    allow_handshake: bool,
) -> Result<(), String> {
    let restore_required = runtime
        .peer_routes
        .get(contact_id)
        .is_some_and(|route| route.resume_required)
        && !active_kktp_binding(runtime, contact_id);
    if !restore_required {
        return Ok(());
    }
    if !allow_handshake {
        return Err(
            "Secure peer transport is restoring after restart; no new handshake was started".into(),
        );
    }
    crate::hydra_commands::prepare_restart_successor(runtime, contact_id)?;
    Ok(())
}

pub(crate) fn authenticated_destination(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    missing: &str,
) -> Result<String, String> {
    runtime
        .peer_routes
        .get(contact_id)
        .map(|route| route.kaspa_address.clone())
        .ok_or_else(|| missing.to_string())
}

pub(crate) fn prepared_durable_message(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<crate::hydra_commands::PreparedMailbox, String> {
    if let Some(prepared) =
        cached_durable_message(runtime, contact_id, message_id, "msg", body, stego_profile)?
    {
        return Ok(prepared);
    }
    validate_new_durable_message(runtime, contact_id, message_id)?;
    let prepared = crate::hydra_commands::prepare_kktp_mailbox(
        runtime,
        contact_id,
        message_id,
        body,
        stego_profile,
    )?;
    remember_prepared_delivery(
        runtime,
        contact_id,
        message_id,
        "msg",
        body,
        stego_profile,
        &prepared,
    );
    checkpoint_prepared_delivery(runtime, contact_id, message_id)?;
    Ok(prepared)
}

fn cached_durable_message(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    kind: &str,
    body: &str,
    stego_profile: &str,
) -> Result<Option<crate::hydra_commands::PreparedMailbox>, String> {
    let Some(cached) = runtime.prepared_kktp_deliveries.get(message_id) else {
        return Ok(None);
    };
    if cached.contact_id != contact_id
        || cached.kind != kind
        || cached.body != body
        || cached.stego_profile != stego_profile
    {
        return Err(
            "The logical KKTP message id is already bound to different prepared content".into(),
        );
    }
    Ok(Some(crate::hydra_commands::PreparedMailbox {
        payloads_hex: cached.payloads_hex.clone(),
    }))
}

fn validate_new_durable_message(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let peer_has_pending = runtime
        .prepared_kktp_deliveries
        .iter()
        .any(|(id, delivery)| id != message_id && delivery.contact_id == contact_id);
    if peer_has_pending {
        return Err("A previous secure message for this peer is still awaiting Kaspa submission; retry that exact message before sending another so the authenticated sequence cannot develop a gap".into());
    }
    if runtime.prepared_kktp_deliveries.len() >= crate::hydra_commands::MAX_PREPARED_KKTP_DELIVERIES
    {
        return Err("Too many unresolved KKTP message submissions are retained in memory; retry failed sends or restart the session".into());
    }
    Ok(())
}

fn remember_prepared_delivery(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    kind: &str,
    body: &str,
    stego_profile: &str,
    prepared: &crate::hydra_commands::PreparedMailbox,
) {
    runtime.prepared_kktp_deliveries.insert(
        message_id.to_owned(),
        crate::hydra_commands::PreparedKktpDelivery {
            contact_id: contact_id.to_owned(),
            kind: kind.to_owned(),
            body: body.to_owned(),
            stego_profile: stego_profile.to_owned(),
            payloads_hex: prepared.payloads_hex.clone(),
        },
    );
}

fn checkpoint_prepared_delivery(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let Err(error) = crate::hydra_commands::persist_transport_state(runtime) else {
        return Ok(());
    };
    runtime.prepared_kktp_deliveries.remove(message_id);
    let reset_error =
        crate::hydra_commands::reset_kktp_after_unsent_advance(runtime, contact_id).err();
    Err(match reset_error {
        Some(reset_error) => format!(
            "Could not checkpoint the prepared secure message ({error}); the session was retired in memory but persisting that retirement also failed: {reset_error}"
        ),
        None => format!(
            "Could not checkpoint the prepared secure message ({error}); the secure session was retired before publication so no sequence gap can be reused"
        ),
    })
}

pub(crate) fn prepare_active_send(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
    reuse_change: bool,
    allow_handshake: bool,
) -> Result<PreparedSend, String> {
    if !active_kktp_binding(runtime, contact_id) {
        return Err(if allow_handshake {
            "active HYDRA session is missing its KKTP binding".into()
        } else {
            "Secure peer transport is not active; no new handshake was started".into()
        });
    }
    let prepared = if reuse_change {
        crate::hydra_commands::prepare_realtime_mailbox(runtime, contact_id, message_id, body)?
    } else {
        prepared_durable_message(runtime, contact_id, message_id, body, stego_profile)?
    };
    runtime.pending_outbound.remove(contact_id);
    runtime.prepared_completion.remove(contact_id);
    Ok(PreparedSend {
        payloads: decode_mailbox_payloads(&prepared.payloads_hex)?,
        pending_id: None,
        destination: authenticated_destination(
            runtime,
            contact_id,
            "active HYDRA session has no authenticated Kaspa peer route",
        )?,
    })
}

mod reactions;
pub(crate) use reactions::prepare_active_reaction_send;

pub(crate) fn responder_handshake_waiting(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
) -> bool {
    runtime
        .kktp_sessions
        .get(contact_id)
        .is_some_and(|binding| {
            binding.role == crate::hydra_commands::KktpRole::Responder
                && matches!(
                    binding.state,
                    crate::hydra_commands::KktpSessionState::Discovered
                        | crate::hydra_commands::KktpSessionState::Handshake
                )
        })
}

pub(crate) fn initiator_sid(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
) -> String {
    runtime
        .kktp_sessions
        .get(contact_id)
        .filter(|binding| {
            binding.role == crate::hydra_commands::KktpRole::Initiator
                && binding.state == crate::hydra_commands::KktpSessionState::Discovered
        })
        .map(|binding| binding.sid.clone())
        .unwrap_or_else(crate::hydra_commands::fresh_kktp_sid)
}
