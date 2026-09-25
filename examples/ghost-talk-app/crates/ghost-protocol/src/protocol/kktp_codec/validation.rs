use super::{
    GhostCallInviteContext, GhostRoomInviteContext, KktpSessionEnd, BASE64, GHOST_KKTP_VERSION,
};
use base64::Engine as _;

pub(crate) fn validate_room_invite_context(
    value: Option<&GhostRoomInviteContext>,
) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.room_id.len() != 32 || !value.room_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk room invite id must be exactly 32 hexadecimal characters".into());
    }
    let name = value.room_name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err("Ghost Talk room invite name must contain 1 to 128 characters".into());
    }
    Ok(())
}

pub(crate) fn validate_call_invite_context(
    value: Option<&GhostCallInviteContext>,
) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    validate_hex(&value.call_id, 32, "Ghost Talk call invite id")?;
    if !matches!(value.action.as_str(), "request" | "decline" | "cancel") {
        return Err("Ghost Talk call bootstrap action must be request, decline, or cancel".into());
    }
    Ok(())
}

pub(crate) fn validate_sid(value: &str) -> Result<(), String> {
    validate_hex(value, 32, "Ghost Talk KKTP sid")
}

pub(crate) fn validate_hex(value: &str, chars: usize, label: &str) -> Result<(), String> {
    if value.len() != chars
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!(
            "{label} must be exactly {chars} lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

pub(crate) fn validate_kktp_session_end_core(value: &KktpSessionEnd) -> Result<(), String> {
    if value.kind != "session_end" || value.version != GHOST_KKTP_VERSION {
        return Err("unsupported Ghost Talk KKTP session_end record".into());
    }
    validate_sid(&value.sid)?;
    validate_hex(&value.initiator_hydra_id, 64, "KKTP initiator HYDRA id")?;
    validate_hex(&value.responder_hydra_id, 64, "KKTP responder HYDRA id")?;
    validate_hex(
        &value.sender_hydra_id,
        64,
        "KKTP session_end sender HYDRA id",
    )?;
    if value.initiator_hydra_id == value.responder_hydra_id {
        return Err("KKTP session_end participants must be distinct".into());
    }
    if value.sender_hydra_id != value.initiator_hydra_id
        && value.sender_hydra_id != value.responder_hydra_id
    {
        return Err("KKTP session_end sender is not a session participant".into());
    }
    if value.sender_kaspa_address.trim().is_empty()
        || value.recipient_kaspa_address.trim().is_empty()
    {
        return Err("KKTP session_end routing address is empty".into());
    }
    if value.reason.is_empty()
        || value.reason.len() > 64
        || value.reason.chars().any(char::is_control)
    {
        return Err("KKTP session_end reason is invalid".into());
    }
    Ok(())
}

pub(crate) fn validate_kktp_session_end(value: &KktpSessionEnd) -> Result<(), String> {
    validate_kktp_session_end_core(value)?;
    let pq = BASE64
        .decode(&value.pq_sig_b64)
        .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
    if pq.is_empty() || pq.len() > 8 * 1024 {
        return Err("KKTP session_end PQ signature size is invalid".into());
    }
    validate_hex(&value.sig, 128, "KKTP session_end Kaspa signature")
}
