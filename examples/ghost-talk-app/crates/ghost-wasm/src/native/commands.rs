use super::invoke::{
    invoke, invoke_unit, json, BroadcastResult, MailboxSendResult, Profile, PublicGhostProfile,
    PublishedGhostDescriptor, ResolvedGhostPeer, WalletProjection, WalletRecovery,
};

mod mailbox_request;
mod wallet;
pub use wallet::{
    broadcast_signer_send, consolidate_kaspa, gather_wallet_history, lock_profile,
    prepare_signer_send, reveal_recovery, send_kaspa, unlock_wallet,
};

pub async fn resolve_peer(
    profile: &Profile,
    password: &str,
    target: &str,
) -> Result<ResolvedGhostPeer, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    let identity = profile
        .hydra_identity_id
        .as_deref()
        .ok_or_else(|| "HYDRA identity is unavailable".to_string())?;
    invoke(
        "resolve_ghost_peer",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "target": target,
            "network": wallet.public.network,
            "restEndpoint": wallet.rest_endpoint,
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn fetch_verified_media(
    reference: &ghost_media::MediaReference,
) -> Result<ghost_api::VerifiedMedia, String> {
    invoke("media_fetch_verified", json!({ "reference": reference })).await
}

pub async fn lookup_public_profile(
    profile: &Profile,
    target: &str,
) -> Result<Option<PublicGhostProfile>, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "lookup_ghost_profile",
        json!({
            "target": target,
            "network": wallet.public.network,
            "restEndpoint": wallet.rest_endpoint,
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn publish_descriptor(
    profile: &Profile,
    password: &str,
    discoverable: bool,
) -> Result<PublishedGhostDescriptor, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    let identity = profile
        .hydra_identity_id
        .as_deref()
        .ok_or_else(|| "HYDRA identity is unavailable".to_string())?;
    let interests = profile
        .settings
        .public_interests
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    invoke(
        "publish_ghost_descriptor",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "displayName": profile.label,
            "username": profile.settings.public_username,
            "description": profile.settings.public_description,
            "interests": interests,
            "avatar": profile.public_avatar(),
            "discoverable": discoverable,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn send_contact_request(
    profile: &Profile,
    password: &str,
    destination: &str,
    request_id: &str,
) -> Result<MailboxSendResult, String> {
    let context =
        mailbox_request::MailboxRequestContext::from_profile(profile, password, destination)?;
    invoke(
        "mailbox_send_contact_request",
        context.contact_request(request_id, None, None),
    )
    .await
}

pub async fn send_call_signal(
    profile: &Profile,
    password: &str,
    destination: &str,
    signal_id: &str,
    call_id: &str,
    action: &str,
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
        "mailbox_send_call_signal",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "senderDisplayName": profile.label,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "destination": destination,
            "signalId": signal_id,
            "callId": call_id,
            "action": action,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn send_call_contact_request(
    profile: &Profile,
    password: &str,
    destination: &str,
    request_id: &str,
    call_id: &str,
) -> Result<MailboxSendResult, String> {
    let context =
        mailbox_request::MailboxRequestContext::from_profile(profile, password, destination)?;
    invoke(
        "mailbox_send_contact_request",
        context.contact_request(request_id, None, Some(call_id)),
    )
    .await
}

pub async fn send_room_contact_request(
    profile: &Profile,
    password: &str,
    destination: &str,
    request_id: &str,
    room_id: &str,
    room_name: &str,
) -> Result<MailboxSendResult, String> {
    let context =
        mailbox_request::MailboxRequestContext::from_profile(profile, password, destination)?;
    invoke(
        "mailbox_send_contact_request",
        context.contact_request(request_id, Some((room_id, room_name)), None),
    )
    .await
}

pub async fn send_contact_accept(
    profile: &Profile,
    password: &str,
    signed_request_hex: &str,
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
        "mailbox_send_contact_accept",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "senderDisplayName": profile.label,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "signedRequestHex": signed_request_hex,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

mod mailbox;
pub use mailbox::{
    send_call_control_message, send_mailbox_message, send_mailbox_message_existing_session,
    send_mailbox_reaction,
};
