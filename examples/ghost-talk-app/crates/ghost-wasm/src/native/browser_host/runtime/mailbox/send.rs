//! Outbound mailbox commands.

mod accept;
mod carriers;
mod message;

use ghost_api::MailboxSendResult;
use serde_json::Value;

use super::common::BrowserWallet;
use crate::native::browser_host::runtime::{
    secure_transport::with_hydra_runtime,
    session::{self, Binding, SessionState},
};

pub(in crate::native::browser_host) use message::first_message;

pub(super) async fn invoke(command: &str, args: &Value) -> Result<MailboxSendResult, String> {
    match command {
        "mailbox_send_contact_accept" | "mailbox_send_message" | "mailbox_send_control" => {
            invoke_session(command, args).await
        }
        "mailbox_send_recovery_offer"
        | "mailbox_retry_handshake_finish"
        | "mailbox_send_delivery_ack" => invoke_recovery(command, args).await,
        _ => invoke_signals(command, args).await,
    }
}

async fn invoke_session(command: &str, args: &Value) -> Result<MailboxSendResult, String> {
    match command {
        "mailbox_send_contact_accept" => accept::send_contact_accept(args).await,
        "mailbox_send_message" => message::send_message(args).await,
        _ => carriers::send_control(args).await,
    }
}

async fn invoke_recovery(command: &str, args: &Value) -> Result<MailboxSendResult, String> {
    match command {
        "mailbox_send_recovery_offer" => carriers::send_recovery_offer(args).await,
        "mailbox_retry_handshake_finish" => carriers::retry_handshake_finish(args).await,
        _ => carriers::send_delivery_ack(args).await,
    }
}

async fn invoke_signals(command: &str, args: &Value) -> Result<MailboxSendResult, String> {
    match command {
        "mailbox_send_realtime_carrier" => carriers::send_realtime_carrier(args).await,
        "mailbox_send_session_end" => super::control::send_session_end(args).await,
        "mailbox_send_call_signal" => super::control::send_call_signal(args).await,
        _ => Err(format!("unknown browser mailbox send command: {command}")),
    }
}

pub(in crate::native::browser_host) fn private_descriptor(
    profile: &str,
    identity: &str,
    wallet: &BrowserWallet,
    label: &str,
) -> Result<ghost_protocol::GhostContactDescriptor, String> {
    let card = with_hydra_runtime(profile, |hydra| hydra.contact_card())?;
    ghost_kaspa::build_private_descriptor(&wallet.secret, &wallet.public, &card, identity, label)
}

pub(super) fn active_binding(profile: &str, peer: &str) -> Result<Binding, String> {
    session::with(profile, |runtime| {
        let binding = runtime
            .sessions
            .get(peer)
            .cloned()
            .ok_or_else(|| "KKTP session binding is missing for this peer".to_string())?;
        if binding.state != SessionState::Active {
            return Err("KKTP session is not active for this peer".into());
        }
        Ok(binding)
    })
}

pub(in crate::native::browser_host) fn ensure_identity(
    profile: &str,
    identity: &str,
) -> Result<(), String> {
    session::with(profile, |runtime| {
        if runtime.identity_id == identity {
            Ok(())
        } else {
            Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into())
        }
    })
}

pub(in crate::native::browser_host) fn validate_route(
    profile: &str,
    peer: &str,
    destination: &str,
) -> Result<(), String> {
    session::with(profile, |runtime| {
        let route = runtime
            .routes
            .get(peer)
            .ok_or_else(|| "secure peer has no verified Kaspa route".to_string())?;
        if route.kaspa_address != destination {
            return Err("mailbox destination does not match the verified peer route".into());
        }
        if runtime.blocked.contains(peer) {
            return Err("secure peer transport is blocked".into());
        }
        Ok(())
    })
}
