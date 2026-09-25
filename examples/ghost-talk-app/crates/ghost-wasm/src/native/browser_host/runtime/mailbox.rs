use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraIncomingRequestProjection, HydraMailboxResult, MailboxSendResult};
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use serde_json::Value;

use super::super::{
    kaspa::{endpoint_override, profile_portal},
    support::util::{required, required_str, to_value},
    HYDRA_RUNTIMES,
};

pub(in crate::native::browser_host) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "mailbox_send_contact_request" => to_value(send_contact_request(args).await?),
        "hydra_receive_mailbox" => super::mailbox_receive::receive(args),
        _ => Err(format!("unknown browser mailbox command: {command}")),
    }
}

pub(in crate::native::browser_host) async fn send_contact_request(args: &Value) -> Result<MailboxSendResult, String> {
    let profile_id = required_str(args, "profileId")?;
    let password = required_str(args, "password")?;
    let identity_id = required_str(args, "identityId")?;
    let destination = required_str(args, "destination")?;
    ghost_kaspa::validate_destination(destination)?;
    let request_id = required_str(args, "requestId")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let projection: crate::model::WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let secret: WalletSecret = ghost_storage::open_json(password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    let request = build_contact_request(args, &secret, &public)?;
    let frames = ghost_protocol::fragment(ghost_core::Id128::new_random(), &request.encode()?)?;
    let portal = profile_portal(profile_id, &public, endpoint_override(args, "wrpcEndpoint")).await?;
    let (transaction_id, public) = send_frames(&portal, &secret, public, destination, frames).await?;
    super::session::remember_contact_request(profile_id, request_id)?;
    Ok(MailboxSendResult {
        transaction_id,
        fee_sompi: "0".into(),
        mailbox_output_sompi: "0".into(),
        public: public.projection(),
        pending_handshake: true,
        pending_id: Some(request_id.to_owned()),
    })
}

fn build_contact_request(
    args: &Value,
    secret: &WalletSecret,
    public: &WalletPublic,
) -> Result<ghost_protocol::GhostContactRequest, String> {
    let profile_id = required_str(args, "profileId")?;
    let identity_id = required_str(args, "identityId")?;
    let descriptor = ghost_kaspa::build_private_descriptor(
        secret,
        public,
        &contact_card(profile_id, identity_id)?,
        identity_id,
        required_str(args, "senderDisplayName")?,
    )?;
    let mut request = ghost_protocol::GhostContactRequest {
        version: ghost_protocol::GHOST_KKTP_VERSION,
        request_id: required_str(args, "requestId")?.to_owned(),
        recipient_kaspa_address: required_str(args, "destination")?.to_owned(),
        sender: descriptor,
        room_invite: room_invite(args)?,
        call_invite: call_invite(args)?,
        signature_hex: String::new(),
    };
    let mut signing_key = ghost_kaspa::wallet::receive_private_key(secret, 0)?;
    let signed = ghost_kaspa::sign_contact_request(&mut request, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    request.encode()?;
    Ok(request)
}

async fn send_frames(
    portal: &ghost_kaspa::PortalFacade,
    secret: &WalletSecret,
    mut public: WalletPublic,
    destination: &str,
    frames: Vec<Vec<u8>>,
) -> Result<(String, WalletPublic), String> {
    let mut transaction_id = String::new();
    for payload in frames {
        let sent = ghost_kaspa::wallet::send_payload(portal, secret, &public, destination, 0, &payload).await?;
        transaction_id = sent.transaction_id;
        public = sent.public;
    }
    Ok((transaction_id, public))
}

pub(in crate::native::browser_host) fn receive_contact_request(
    profile_id: &str,
    identity_id: &str,
    local_addresses: &[String],
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    let request = ghost_protocol::GhostContactRequest::decode(envelope)?;
    ghost_kaspa::verify_contact_request(&request)?;
    if !local_addresses.iter().any(|address| address == &request.recipient_kaspa_address) {
        return Err("contact request is not addressed to this Ghost Talk wallet".into());
    }
    let card = BASE64.decode(&request.sender.hydra_contact_card_b64)
        .map_err(|_| "contact-request HYDRA contact card is not valid base64".to_string())?;
    let peer_hydra_id = preview_contact(profile_id, identity_id, &card, &request)?;
    Ok(HydraMailboxResult {
        incoming_request: Some(HydraIncomingRequestProjection {
            request_id: request.request_id,
            peer_address: request.sender.kaspa_address,
            local_address: request.recipient_kaspa_address,
            peer_label: request.sender.display_name,
            peer_hydra_id,
            signed_request_hex: hex::encode(envelope),
            room_invite: request.room_invite.map(|invite| ghost_chat::RoomInviteMeta { room_id: invite.room_id, room_name: invite.room_name }),
            call_id: request.call_invite.as_ref().map(|invite| invite.call_id.clone()),
            call_action: request.call_invite.map(|invite| invite.action),
        }),
        discard: true,
        ..Default::default()
    })
}

fn preview_contact(
    profile_id: &str,
    identity_id: &str,
    card: &[u8],
    request: &ghost_protocol::GhostContactRequest,
) -> Result<String, String> {
    HYDRA_RUNTIMES.with(|runtimes| {
        let runtimes = runtimes.borrow();
        let hydra = runtimes
            .get(profile_id)
            .ok_or_else(|| "HYDRA profile must be unlocked before receiving mail".to_string())?;
        let local = hydra
            .list_identities()
            .into_iter()
            .find(|identity| identity.id == identity_id && identity.unlocked)
            .ok_or_else(|| "selected HYDRA identity is not unlocked".to_string())?;
        let contact = hydra.preview_contact(card)?;
        if request.sender.hydra_identity_id != contact.handle {
            return Err("contact-request HYDRA identity does not match its authenticated contact card".into());
        }
        if local.id == contact.handle {
            return Err("cannot receive a Ghost Talk contact request from this identity itself".into());
        }
        Ok(contact.handle)
    })
}

fn contact_card(profile_id: &str, identity_id: &str) -> Result<Vec<u8>, String> {
    HYDRA_RUNTIMES.with(|runtimes| {
        let runtimes = runtimes.borrow();
        let hydra = runtimes
            .get(profile_id)
            .ok_or_else(|| "HYDRA profile must be unlocked before sending mail".to_string())?;
        let unlocked = hydra
            .list_identities()
            .into_iter()
            .any(|identity| identity.id == identity_id && identity.unlocked);
        if !unlocked {
            return Err("selected HYDRA identity is not unlocked".into());
        }
        hydra.contact_card()
    })
}

fn room_invite(args: &Value) -> Result<Option<ghost_protocol::GhostRoomInviteContext>, String> {
    match (
        args.get("roomId").and_then(Value::as_str),
        args.get("roomName").and_then(Value::as_str),
    ) {
        (None, None) => Ok(None),
        (Some(room_id), Some(room_name)) => Ok(Some(ghost_protocol::GhostRoomInviteContext {
            room_id: room_id.to_owned(),
            room_name: room_name.to_owned(),
        })),
        _ => Err("room invite bootstrap requires both room id and room name".into()),
    }
}

fn call_invite(args: &Value) -> Result<Option<ghost_protocol::GhostCallInviteContext>, String> {
    let Some(call_id) = args.get("callId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let action = args
        .get("callAction")
        .and_then(Value::as_str)
        .unwrap_or("request");
    Ok(Some(ghost_protocol::GhostCallInviteContext {
        call_id: call_id.to_owned(),
        action: action.to_owned(),
    }))
}
