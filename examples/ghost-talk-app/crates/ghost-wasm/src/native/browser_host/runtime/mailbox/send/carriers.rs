use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::MailboxSendResult;
use ghost_protocol::GhostDeliveryAck;
use serde_json::Value;
use zeroize::Zeroize;

use super::{ensure_identity, validate_route};
use crate::native::browser_host::{
    runtime::{
        handshake,
        mailbox::common::{decode_payloads, frame, BrowserWallet},
        session::{self, SessionState},
    },
    support::util::{required, required_str},
};

pub(super) async fn send_control(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let destination = required_str(args, "destination")?.to_owned();
    let values: Vec<String> = required(args, "payloadsHex")?;
    let payloads = decode_payloads(&values)?;
    let pending = args
        .get("completesPendingId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let sent = wallet
        .send(args, &destination, payloads, false, None)
        .await?;
    if let Some(pending_id) = pending {
        session::with_mut(&wallet.profile_id, |runtime| {
            runtime
                .pending_outbound
                .retain(|_, pending| pending.id != pending_id);
            Ok(())
        })?;
    }
    Ok(sent)
}

pub(super) async fn send_recovery_offer(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let peer = required_str(args, "contactId")?;
    let destination = required_str(args, "destination")?;
    let offer = hex::decode(required_str(args, "offerHex")?)
        .map_err(|_| "HYDRA recovery offer is not valid hex".to_string())?;
    let binding = session::with(&profile, |runtime| {
        runtime
            .sessions
            .get(peer)
            .cloned()
            .ok_or_else(|| "KKTP recovery session binding is missing".to_string())
    })?;
    let carrier = handshake::control(&profile, &binding, "pq_init", &offer, None)?;
    wallet
        .send(args, destination, frame(&carrier)?, true, None)
        .await
}

pub(super) async fn retry_handshake_finish(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let identity = required_str(args, "identityId")?;
    let peer = required_str(args, "contactId")?;
    let message_id = required_str(args, "messageId")?;
    ghost_protocol::validate_kktp_message_id(message_id)?;
    ensure_identity(&wallet.profile_id, identity)?;
    let prepared = session::with(&wallet.profile_id, |runtime| {
        runtime
            .prepared_completion
            .get(peer)
            .cloned()
            .ok_or_else(|| "no peer-unacknowledged KKTP FINISH is retained for retry".to_string())
    })?;
    if prepared.message_id != message_id {
        return Err("retained KKTP FINISH does not match this chat message".into());
    }
    let payloads = decode_payloads(&prepared.payloads_hex)?;
    wallet
        .send(args, &prepared.destination, payloads, true, None)
        .await
}

pub(super) async fn send_delivery_ack(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let identity = required_str(args, "identityId")?;
    let peer = required_str(args, "destinationHydraId")?;
    let destination = required_str(args, "destination")?;
    validate_route(&wallet.profile_id, peer, destination)?;
    let mut ack = GhostDeliveryAck {
        version: 1,
        signer_kaspa_address: wallet
            .public
            .receive_addresses
            .first()
            .cloned()
            .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?,
        signer_hydra_id: identity.to_owned(),
        destination_hydra_id: peer.to_owned(),
        message_id: required_str(args, "messageId")?.to_owned(),
        signature_hex: String::new(),
    };
    let mut key = ghost_kaspa::wallet::receive_private_key(&wallet.secret, 0)?;
    let result = ghost_kaspa::sign_delivery_ack(&mut ack, &key);
    key.zeroize();
    result?;
    wallet
        .send(args, destination, frame(&ack.encode()?)?, false, None)
        .await
}

pub(super) async fn send_realtime_carrier(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let peer = required_str(args, "contactId")?;
    let destination = required_str(args, "destination")?;
    validate_route(&profile, peer, destination)?;
    let carrier = BASE64
        .decode(required_str(args, "carrierB64")?)
        .map_err(|_| "GTR1 realtime carrier is not valid base64".to_string())?;
    let decoded =
        ghost_realtime::Gtr1Envelope::decode(&carrier).map_err(|error| error.to_string())?;
    let (identity, sid) = session::with(&profile, |runtime| {
        let binding = runtime
            .sessions
            .get(peer)
            .ok_or_else(|| "realtime carrier has no active KKTP session".to_string())?;
        if binding.state != SessionState::Active {
            return Err("realtime carrier requires an active KKTP session".into());
        }
        Ok((runtime.identity_id.clone(), binding.sid.clone()))
    })?;
    if decoded.sender_hex() != identity || decoded.sid_hex() != sid {
        return Err("GTR1 realtime carrier does not match the active authenticated session".into());
    }
    wallet
        .send(args, destination, frame(&carrier)?, true, None)
        .await
}
