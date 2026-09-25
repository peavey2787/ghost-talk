use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::MailboxSendResult;
use ghost_protocol::{GhostCallSignal, KktpSessionEnd, GHOST_KKTP_VERSION};
use serde_json::Value;
use zeroize::Zeroize;

use super::{secure_transport::with_hydra_runtime, mailbox_common::{frame, BrowserWallet}, mailbox_send::{ensure_identity, private_descriptor, validate_route}, session::{self, Role}};
use super::super::support::util::required_str;

pub(in crate::native::browser_host) async fn send_call_signal(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let identity = required_str(args, "identityId")?;
    ensure_identity(&profile, identity)?;
    let destination = required_str(args, "destination")?;
    let signal_id = required_str(args, "signalId")?;
    let call_id = required_str(args, "callId")?;
    let action = required_str(args, "action")?;
    validate_id(signal_id, "Ghost Talk call signal id")?;
    validate_id(call_id, "Ghost Talk call id")?;
    if !matches!(action, "request" | "accept" | "decline" | "cancel") { return Err("Ghost Talk call signal action must be request, accept, decline, or cancel".into()); }
    let descriptor = private_descriptor(&profile, identity, &wallet, required_str(args, "senderDisplayName")?)?;
    let mut signal = GhostCallSignal { kind: "call_signal".into(), version: GHOST_KKTP_VERSION, signal_id: signal_id.into(), call_id: call_id.into(), action: action.into(), recipient_kaspa_address: destination.into(), sender: descriptor, signature_hex: String::new() };
    let mut key = ghost_kaspa::wallet::receive_private_key(&wallet.secret, 0)?;
    let signed = ghost_kaspa::sign_call_signal(&mut signal, &key); key.zeroize(); signed?;
    wallet.send(args, destination, frame(&signal.encode()?)?, false, None).await
}

pub(in crate::native::browser_host) async fn send_session_end(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let identity = required_str(args, "identityId")?;
    let peer = required_str(args, "contactId")?;
    let destination = required_str(args, "destination")?;
    ensure_identity(&profile, identity)?;
    validate_route(&profile, peer, destination)?;
    let binding = session::with(&profile, |runtime| runtime.sessions.get(peer).cloned().ok_or_else(|| "KKTP session binding is missing for this peer".to_string()))?;
    if let Some(expected) = args.get("expectedSessionSid").and_then(Value::as_str) { if binding.sid != expected { return Err("stale chat leave no longer matches the active KKTP session".into()); } }
    let (initiator, responder) = match binding.role { Role::Initiator => (identity.to_owned(), peer.to_owned()), Role::Responder => (peer.to_owned(), identity.to_owned()) };
    let sender = wallet.public.receive_addresses.first().cloned().ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let mut end = KktpSessionEnd { kind: "session_end".into(), version: GHOST_KKTP_VERSION, sid: binding.sid, initiator_hydra_id: initiator, responder_hydra_id: responder, sender_hydra_id: identity.to_owned(), sender_kaspa_address: sender, recipient_kaspa_address: destination.to_owned(), reason: "left".into(), pq_sig_b64: String::new(), sig: String::new() };
    let pq = with_hydra_runtime(&profile, |hydra| hydra.sign_application_context(&end.pq_signing_bytes()?))?;
    end.pq_sig_b64 = BASE64.encode(pq);
    let mut key = ghost_kaspa::wallet::receive_private_key(&wallet.secret, 0)?;
    let signed = ghost_kaspa::sign_kktp_session_end(&mut end, &key); key.zeroize(); signed?;
    wallet.send(args, destination, frame(&end.encode()?)?, false, None).await
}

fn validate_id(value: &str, label: &str) -> Result<(), String> {
    let valid = value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
    valid.then_some(()).ok_or_else(|| format!("{label} must be exactly 32 lowercase hexadecimal characters"))
}
