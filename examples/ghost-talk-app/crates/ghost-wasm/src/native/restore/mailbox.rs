use super::{
    invoke, json, HydraControlProjection, HydraMailboxResult, HydraRecoveryProjection,
    MailboxSendResult, Profile,
};
use crate::native::invoke::Value;

pub async fn send_mailbox_control(
    profile: &Profile,
    password: &str,
    control: &HydraControlProjection,
) -> Result<MailboxSendResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "mailbox_send_control",
        json!({
            "profileId": profile.id,
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "destination": control.destination,
            "payloadsHex": control.payloads_hex,
            "completesPendingId": control.completes_pending_id,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn receive_mailbox(
    profile: &Profile,
    password: &str,
    envelope_hex: &str,
) -> Result<HydraMailboxResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    let identity = profile
        .hydra_identity_id
        .as_deref()
        .ok_or_else(|| "HYDRA identity is unavailable".to_string())?;
    let local_addresses = wallet
        .public
        .receive_addresses
        .iter()
        .chain(wallet.public.change_addresses.iter())
        .cloned()
        .collect::<Vec<_>>();
    let active_sessions = canonical_active_sessions(profile);
    invoke(
        "hydra_receive_mailbox",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "envelopeHex": envelope_hex,
            "localKaspaAddresses": local_addresses,
            "activeSessions": active_sessions,
        }),
    )
    .await
}

/// Admit exactly one persisted SID per authenticated peer. Direct-chat and
/// room-only surfaces may share one transport, so stale duplicate SIDs must not
/// remain valid after restart.
fn canonical_active_sessions(profile: &Profile) -> Vec<Value> {
    let mut canonical_sessions = std::collections::HashMap::<String, (u8, String)>::new();
    for chat in profile
        .chats
        .iter()
        .filter(|chat| !chat.left() && !chat.peer_left())
    {
        let (Some(peer), Some(sid)) = (chat.peer_hydra_handle(), chat.session_sid()) else {
            continue;
        };
        let priority = if chat.bootstrap_complete() {
            3
        } else if chat.transport_restore_pending() {
            2
        } else {
            1
        };
        let replace = canonical_sessions
            .get(peer)
            .is_none_or(|(current_priority, _)| priority > *current_priority);
        if replace {
            canonical_sessions.insert(peer.to_owned(), (priority, sid.to_owned()));
        }
    }
    canonical_sessions
        .into_iter()
        .map(|(peer_hydra_id, (_, sid))| {
            json!({
                "peer_hydra_id": peer_hydra_id,
                "sid": sid,
            })
        })
        .collect()
}

pub async fn send_recovery_offer(
    profile: &Profile,
    password: &str,
    recovery: &HydraRecoveryProjection,
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
        "mailbox_send_recovery_offer",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "contactId": recovery.peer_hydra_id,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "destination": recovery.destination,
            "offerHex": recovery.offer_hex,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn send_delivery_ack(
    profile: &Profile,
    password: &str,
    destination: &str,
    destination_hydra_id: &str,
    message_id: &str,
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
        "mailbox_send_delivery_ack",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "destination": destination,
            "destinationHydraId": destination_hydra_id,
            "messageId": message_id,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}
