pub(crate) struct ContactBootstrap {
    pub(crate) room_invite: Option<ghost_protocol::GhostRoomInviteContext>,
    pub(crate) call_invite: Option<ghost_protocol::GhostCallInviteContext>,
    pub(crate) reuse_change: bool,
}

pub(crate) fn validate_contact_request_id(request_id: &str) -> Result<(), String> {
    if request_id.len() != 32 || !request_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            "Ghost Talk contact-request id must be exactly 32 hexadecimal characters".into(),
        );
    }
    Ok(())
}

pub(crate) fn contact_bootstrap(
    room_id: Option<String>,
    room_name: Option<String>,
    call_id: Option<String>,
    call_action: Option<String>,
) -> Result<ContactBootstrap, String> {
    let room_invite = room_bootstrap(room_id, room_name)?;
    let call_invite = call_bootstrap(call_id, call_action)?;
    if room_invite.is_some() && call_invite.is_some() {
        return Err("contact bootstrap cannot be both a room invite and a voice call".into());
    }
    Ok(ContactBootstrap {
        reuse_change: call_invite.is_some(),
        room_invite,
        call_invite,
    })
}

fn room_bootstrap(
    room_id: Option<String>,
    room_name: Option<String>,
) -> Result<Option<ghost_protocol::GhostRoomInviteContext>, String> {
    match (room_id, room_name) {
        (None, None) => Ok(None),
        (Some(room_id), Some(room_name)) => Ok(Some(ghost_protocol::GhostRoomInviteContext {
            room_id,
            room_name,
        })),
        _ => Err("room invite bootstrap requires both room id and room name".into()),
    }
}

fn call_bootstrap(
    call_id: Option<String>,
    call_action: Option<String>,
) -> Result<Option<ghost_protocol::GhostCallInviteContext>, String> {
    match (call_id, call_action) {
        (None, None) => Ok(None),
        (Some(call_id), action) => Ok(Some(ghost_protocol::GhostCallInviteContext {
            call_id,
            action: action.unwrap_or_else(|| "request".into()),
        })),
        (None, Some(_)) => Err("call bootstrap action requires a call id".into()),
    }
}

pub(crate) fn signed_contact_request(
    secret: &ghost_kaspa::wallet::WalletSecret,
    request_id: String,
    destination: String,
    descriptor: ghost_protocol::GhostContactDescriptor,
    room_invite: Option<ghost_protocol::GhostRoomInviteContext>,
    call_invite: Option<ghost_protocol::GhostCallInviteContext>,
) -> Result<ghost_protocol::GhostContactRequest, String> {
    let mut request = ghost_protocol::GhostContactRequest {
        version: ghost_protocol::GHOST_KKTP_VERSION,
        request_id,
        recipient_kaspa_address: destination,
        sender: descriptor,
        room_invite,
        call_invite,
        signature_hex: String::new(),
    };
    let mut signing_key = ghost_kaspa::wallet::receive_private_key(secret, 0)?;
    let signed = ghost_kaspa::sign_contact_request(&mut request, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    Ok(request)
}

pub(crate) async fn private_descriptor(
    runtime: &std::sync::Arc<tokio::sync::Mutex<crate::hydra_commands::HydraProfileRuntime>>,
    identity_id: &str,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &ghost_kaspa::wallet::WalletPublic,
    sender_display_name: &str,
) -> Result<ghost_protocol::GhostContactDescriptor, String> {
    let runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    let card = runtime.hydra.contact_card()?;
    crate::peer_commands::build_private_descriptor(
        secret,
        public,
        &card,
        identity_id,
        sender_display_name,
    )
}

pub(crate) async fn set_pending_contact_request(
    runtime: &std::sync::Arc<tokio::sync::Mutex<crate::hydra_commands::HydraProfileRuntime>>,
    request_id: &str,
    pending: bool,
) -> Result<(), String> {
    let mut runtime = runtime.lock().await;
    let sid = request_id.to_ascii_lowercase();
    if pending {
        runtime.pending_contact_request_sids.insert(sid);
    } else {
        runtime.pending_contact_request_sids.remove(&sid);
    }
    crate::hydra_commands::persist_transport_state(&runtime)
}

pub(crate) fn validate_call_signal_fields(
    signal_id: &str,
    call_id: &str,
    action: &str,
) -> Result<(), String> {
    validate_lower_hex_id(signal_id, "Ghost Talk call signal id")?;
    validate_lower_hex_id(call_id, "Ghost Talk call id")?;
    if !matches!(action, "request" | "accept" | "decline" | "cancel") {
        return Err(
            "Ghost Talk call signal action must be request, accept, decline, or cancel".into(),
        );
    }
    Ok(())
}

fn validate_lower_hex_id(value: &str, label: &str) -> Result<(), String> {
    let valid = value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{label} must be exactly 32 lowercase hexadecimal characters"
        ))
    }
}

pub(crate) fn signed_call_signal(
    secret: &ghost_kaspa::wallet::WalletSecret,
    descriptor: ghost_protocol::GhostContactDescriptor,
    signal_id: String,
    call_id: String,
    action: String,
    destination: String,
) -> Result<ghost_protocol::GhostCallSignal, String> {
    let mut signal = ghost_protocol::GhostCallSignal {
        kind: "call_signal".into(),
        version: ghost_protocol::GHOST_KKTP_VERSION,
        signal_id,
        call_id,
        action,
        recipient_kaspa_address: destination,
        sender: descriptor,
        signature_hex: String::new(),
    };
    let mut signing_key = ghost_kaspa::wallet::receive_private_key(secret, 0)?;
    let signed = ghost_kaspa::sign_call_signal(&mut signal, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    Ok(signal)
}

pub(crate) async fn accepted_contact_request(
    runtime: &std::sync::Arc<tokio::sync::Mutex<crate::hydra_commands::HydraProfileRuntime>>,
    identity_id: &str,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &ghost_kaspa::wallet::WalletPublic,
    sender_display_name: &str,
    signed_request_hex: &str,
    local_kaspa_addresses: &[String],
) -> Result<
    (
        ghost_protocol::GhostContactRequest,
        ghost_protocol::GhostContactDescriptor,
    ),
    String,
> {
    let mut runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    let request = crate::hydra_commands::accept_signed_contact_request(
        &mut runtime,
        signed_request_hex,
        local_kaspa_addresses,
    )?;
    let card = runtime.hydra.contact_card()?;
    let descriptor = crate::peer_commands::build_private_descriptor(
        secret,
        public,
        &card,
        identity_id,
        sender_display_name,
    )?;
    Ok((request, descriptor))
}

pub(crate) fn signed_contact_accept(
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &ghost_kaspa::wallet::WalletPublic,
    request: ghost_protocol::GhostContactRequest,
    descriptor: ghost_protocol::GhostContactDescriptor,
) -> Result<(ghost_protocol::GhostContactAccept, String, String, bool), String> {
    let call_bootstrap = request.call_invite.is_some();
    let destination = request.sender.kaspa_address.clone();
    let acceptor_kaspa_address = request.recipient_kaspa_address.clone();
    ghost_kaspa::validate_destination(&destination)?;
    ghost_kaspa::validate_destination(&acceptor_kaspa_address)?;
    let mut accepted = ghost_protocol::GhostContactAccept {
        version: ghost_protocol::GHOST_KKTP_VERSION,
        request_id: request.request_id,
        recipient_kaspa_address: destination.clone(),
        acceptor_kaspa_address: acceptor_kaspa_address.clone(),
        responder: descriptor,
        signature_hex: String::new(),
    };
    let mut signing_key =
        ghost_kaspa::wallet::private_key_for_address(secret, public, &acceptor_kaspa_address)?;
    let signed = ghost_kaspa::sign_contact_accept(&mut accepted, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    Ok((
        accepted,
        destination,
        acceptor_kaspa_address,
        call_bootstrap,
    ))
}
