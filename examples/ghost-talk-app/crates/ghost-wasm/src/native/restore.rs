use super::commands::send_mailbox_message;
use super::{
    invoke::{
        invoke, invoke_unit, json, BackupContact, BackupMessage, HydraControlProjection,
        HydraMailboxResult, HydraRealtimeEnvelope, HydraRecoveryProjection,
        HydraSessionBindingProjection, MailboxSendResult, Profile, ProfileBackupPublishResult,
        ProfileBackupRestoreResult, SESSION_RESTORE_BODY,
    },
    transport_restore::apply_surviving_binding,
};
pub(crate) fn apply_restored_peer_binding(
    profile: &mut Profile,
    peer: &str,
    binding: &HydraSessionBindingProjection,
) {
    let chat_ids: Vec<String> = profile
        .chats
        .iter()
        .filter(|chat| !chat.left() && !chat.peer_left() && chat.peer_hydra_handle() == Some(peer))
        .map(|chat| chat.id.clone())
        .collect();
    for chat_id in chat_ids {
        apply_surviving_binding(&mut profile.chats, &chat_id, binding);
    }
}

pub(crate) async fn inspect_restore_job(profile: &mut Profile, peer: &str) -> Result<bool, String> {
    match peer_session_binding(&profile.id, peer).await? {
        Some(binding) => {
            apply_restored_peer_binding(profile, peer, &binding);
            Ok(true)
        }
        None => Ok(false),
    }
}

pub(crate) async fn send_restore_job(
    profile: &mut Profile,
    password: &str,
    peer: &str,
    destination: &str,
    message_id: &str,
) -> Result<(), String> {
    let sent = send_mailbox_message(
        profile,
        password,
        peer,
        destination,
        SESSION_RESTORE_BODY,
        message_id,
    )
    .await?;
    crate::model::WalletStateService::merge_progress(&mut profile.wallet, sent.public.clone());
    Ok(())
}

pub(crate) async fn restore_peer_transport(
    profile: &mut Profile,
    password: &str,
    peer: &str,
    destination: &str,
    message_id: &str,
) -> Result<(), String> {
    if inspect_restore_job(profile, peer).await? {
        return Ok(());
    }
    send_restore_job(profile, password, peer, destination, message_id).await
}

pub async fn resume_profile_sessions(mut profile: Profile, password: &str) -> (Profile, bool) {
    let ids = allocate_restore_ids(&profile);
    assign_restore_ids(&mut profile.chats, &ids);
    let jobs = restore_jobs(&profile);
    let mut had_failure = false;
    for (peer, destination, message_id) in jobs {
        if let Err(error) =
            restore_peer_transport(&mut profile, password, &peer, &destination, &message_id).await
        {
            had_failure = true;
            web_sys::console::warn_1(
                &format!("Ghost Talk secure-session restore deferred for {peer}: {error}").into(),
            );
        }
    }
    (profile, had_failure)
}

pub async fn seal_realtime(
    profile_id: &str,
    contact_id: &str,
    message_id: &str,
    body: &str,
) -> Result<HydraRealtimeEnvelope, String> {
    invoke(
        "hydra_seal_realtime",
        json!({
            "profileId": profile_id,
            "contactId": contact_id,
            "messageId": message_id,
            "body": body,
        }),
    )
    .await
}

pub async fn open_realtime(
    profile_id: &str,
    carrier_b64: &str,
) -> Result<HydraMailboxResult, String> {
    invoke(
        "hydra_open_realtime",
        json!({
            "profileId": profile_id,
            "carrierB64": carrier_b64,
        }),
    )
    .await
}

pub async fn send_realtime_carrier(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    carrier_b64: &str,
) -> Result<MailboxSendResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "mailbox_send_realtime_carrier",
        json!({
            "profileId": profile.id,
            "password": password,
            "contactId": contact_id,
            "destination": destination,
            "carrierB64": carrier_b64,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn peer_session_binding(
    profile_id: &str,
    contact_id: &str,
) -> Result<Option<HydraSessionBindingProjection>, String> {
    invoke(
        "hydra_peer_session_binding",
        json!({ "profileId": profile_id, "contactId": contact_id }),
    )
    .await
}

pub async fn leave_peer(profile_id: &str, contact_id: &str) -> Result<(), String> {
    invoke_unit(
        "hydra_leave_peer",
        json!({
            "profileId": profile_id,
            "contactId": contact_id,
            "expectedSessionSid": serde_json::Value::Null,
        }),
    )
    .await
}

pub async fn leave_peer_session(
    profile_id: &str,
    contact_id: &str,
    expected_session_sid: Option<&str>,
) -> Result<(), String> {
    invoke_unit(
        "hydra_leave_peer",
        json!({
            "profileId": profile_id,
            "contactId": contact_id,
            "expectedSessionSid": expected_session_sid,
        }),
    )
    .await
}

pub async fn rejoin_peer(profile_id: &str, contact_id: &str) -> Result<(), String> {
    invoke_unit(
        "hydra_rejoin_peer",
        json!({ "profileId": profile_id, "contactId": contact_id }),
    )
    .await
}

pub async fn send_session_end(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    expected_session_sid: Option<&str>,
) -> Result<MailboxSendResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    let identity = profile
        .hydra_identity_id
        .as_deref()
        .ok_or_else(|| "HYDRA identity is unavailable".to_string())?;
    invoke(
        "mailbox_send_session_end",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "contactId": contact_id,
            "destination": destination,
            "expectedSessionSid": expected_session_sid,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

mod backup;
mod ids;
mod mailbox;
pub use backup::{publish_profile_backup, restore_profile_backup};
pub(crate) use ids::{allocate_restore_ids, assign_restore_ids, restore_jobs};
pub use mailbox::{receive_mailbox, send_delivery_ack, send_mailbox_control, send_recovery_offer};
