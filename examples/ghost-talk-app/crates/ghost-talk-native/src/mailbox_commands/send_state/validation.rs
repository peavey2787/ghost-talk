pub(crate) fn validate_message_send(
    body: &str,
    message_id: &str,
    stego_profile: &str,
) -> Result<(), String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("message body is empty or exceeds the Ghost Talk event limit".into());
    }
    crate::hydra_commands::parse_stego_profile(stego_profile)?;
    if message_id.len() != 32 || !message_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk message id must be exactly 32 hexadecimal characters".into());
    }
    Ok(())
}

pub(crate) fn validate_send_runtime(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    identity_id: &str,
    contact_id: &str,
) -> Result<(), String> {
    [
        (
            runtime.identity_id == identity_id,
            "selected HYDRA identity does not match the unlocked Ghost Talk ID",
        ),
        (
            !runtime.blocked_peers.contains(contact_id),
            "This peer ended the current chat session. Start a new chat to establish a fresh KKTP SID before sending again.",
        ),
        (
            !runtime.prepared_completion.contains_key(contact_id),
            ghost_domain::send_error::FINISH_AWAITING_ACK,
        ),
        (
            !runtime.prepared_recovery_finish.contains_key(contact_id),
            ghost_domain::send_error::RECOVERY_FINISH_AWAITING_BROADCAST,
        ),
    ]
    .into_iter()
    .find(|(valid, _)| !valid)
    .map(|(_, message)| Err(message.into()))
    .unwrap_or(Ok(()))
}
