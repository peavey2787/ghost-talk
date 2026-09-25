use super::{invoke, json, MailboxSendResult, Profile};

struct MailboxSendMode<'a> {
    allow_handshake: bool,
    reuse_change: bool,
    stego_profile: &'a str,
}

async fn send_mailbox_message_mode(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    body: &str,
    message_id: &str,
    mode: MailboxSendMode<'_>,
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
        "mailbox_send_message",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "contactId": contact_id,
            "destination": destination,
            "body": body,
            "reaction": null,
            "messageId": message_id,
            "stegoProfile": mode.stego_profile,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
            "reuseChange": mode.reuse_change,
            "allowHandshake": mode.allow_handshake,
        }),
    )
    .await
}

pub async fn send_mailbox_message(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    body: &str,
    message_id: &str,
) -> Result<MailboxSendResult, String> {
    send_mailbox_message_mode(
        profile,
        password,
        contact_id,
        destination,
        body,
        message_id,
        MailboxSendMode {
            allow_handshake: true,
            reuse_change: false,
            stego_profile: &profile.settings.stego,
        },
    )
    .await
}

pub async fn send_mailbox_message_existing_session(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    body: &str,
    message_id: &str,
) -> Result<MailboxSendResult, String> {
    send_mailbox_message_mode(
        profile,
        password,
        contact_id,
        destination,
        body,
        message_id,
        MailboxSendMode {
            allow_handshake: false,
            reuse_change: false,
            stego_profile: &profile.settings.stego,
        },
    )
    .await
}

pub async fn send_call_control_message(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    body: &str,
    message_id: &str,
) -> Result<MailboxSendResult, String> {
    send_mailbox_message_mode(
        profile,
        password,
        contact_id,
        destination,
        body,
        message_id,
        MailboxSendMode {
            allow_handshake: true,
            reuse_change: true,
            stego_profile: &profile.settings.stego,
        },
    )
    .await
}

pub async fn send_mailbox_reaction(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    reaction: &ghost_protocol::GhostReactionEvent,
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
        "mailbox_send_message",
        json!({
            "profileId": profile.id,
            "password": password,
            "identityId": identity,
            "contactId": contact_id,
            "destination": destination,
            "body": "",
            "reaction": reaction,
            "messageId": message_id,
            "stegoProfile": profile.settings.stego,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
            "reuseChange": false,
            "allowHandshake": false,
        }),
    )
    .await
}
