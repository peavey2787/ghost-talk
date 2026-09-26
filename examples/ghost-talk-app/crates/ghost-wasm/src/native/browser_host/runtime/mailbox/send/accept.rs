use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::MailboxSendResult;
use ghost_protocol::{GhostContactAccept, GhostContactRequest, GHOST_KKTP_VERSION};
use serde_json::Value;
use zeroize::Zeroize;

use super::{ensure_identity, private_descriptor};
use crate::native::browser_host::{
    runtime::{
        handshake,
        mailbox::common::{frame, BrowserWallet},
        secure_transport::with_hydra_runtime,
        session::{self, Role, SessionState},
    },
    support::util::required_str,
};

pub(super) async fn send_contact_accept(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let identity = required_str(args, "identityId")?;
    ensure_identity(&profile, identity)?;
    let request = verified_request(args, &wallet)?;
    install_requester(&profile, &request)?;
    let descriptor = private_descriptor(
        &profile,
        identity,
        &wallet,
        required_str(args, "senderDisplayName")?,
    )?;
    let destination = request.sender.kaspa_address.clone();
    let acceptor = request.recipient_kaspa_address.clone();
    let mut accepted = GhostContactAccept {
        version: GHOST_KKTP_VERSION,
        request_id: request.request_id,
        recipient_kaspa_address: destination.clone(),
        acceptor_kaspa_address: acceptor.clone(),
        responder: descriptor,
        signature_hex: String::new(),
    };
    let mut key =
        ghost_kaspa::wallet::private_key_for_address(&wallet.secret, &wallet.public, &acceptor)?;
    let signed = ghost_kaspa::sign_contact_accept(&mut accepted, &key);
    key.zeroize();
    signed?;
    wallet
        .send(args, &destination, frame(&accepted.encode()?)?, true, None)
        .await
}

fn verified_request(args: &Value, wallet: &BrowserWallet) -> Result<GhostContactRequest, String> {
    let encoded = hex::decode(required_str(args, "signedRequestHex")?)
        .map_err(|_| "signed Ghost Talk contact request is not valid hex".to_string())?;
    let request = GhostContactRequest::decode(&encoded)?;
    ghost_kaspa::verify_contact_request(&request)?;
    let local = wallet
        .public
        .receive_addresses
        .iter()
        .chain(wallet.public.change_addresses.iter())
        .any(|address| address == &request.recipient_kaspa_address);
    if !local {
        return Err("contact request is not addressed to this Ghost Talk wallet".into());
    }
    Ok(request)
}

fn install_requester(profile: &str, request: &GhostContactRequest) -> Result<(), String> {
    let card = BASE64
        .decode(&request.sender.hydra_contact_card_b64)
        .map_err(|_| "contact-request HYDRA contact card is not valid base64".to_string())?;
    let peer = with_hydra_runtime(profile, |hydra| {
        let preview = hydra.preview_contact(&card)?;
        if preview.handle != request.sender.hydra_identity_id {
            return Err(
                "contact-request HYDRA identity does not match its authenticated contact card"
                    .into(),
            );
        }
        Ok(hydra.add_contact(&card)?.handle)
    })?;
    handshake::reset_peer_crypto(profile, &peer)?;
    let sid = request.request_id.clone();
    session::install(
        profile,
        &peer,
        sid.clone(),
        Role::Responder,
        SessionState::Discovered,
    )?;
    session::remember_route(
        profile,
        &peer,
        request.sender.kaspa_address.clone(),
        request.sender.display_name.clone(),
        sid,
        Role::Responder,
    )
    .map(|_| ())
}
