use ghost_api::MailboxSendResult;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use serde_json::Value;

use super::super::{kaspa::{endpoint_override, profile_portal}, support::util::{required, required_str}};

pub(in crate::native::browser_host) struct BrowserWallet {
    pub(in crate::native::browser_host) profile_id: String,
    pub(in crate::native::browser_host) secret: WalletSecret,
    pub(in crate::native::browser_host) public: WalletPublic,
}

impl BrowserWallet {
    pub(in crate::native::browser_host) fn open(args: &Value) -> Result<Self, String> {
        let profile_id = required_str(args, "profileId")?.to_owned();
        let password = required_str(args, "password")?;
        let sealed: Vec<u8> = required(args, "sealed")?;
        let projection: crate::model::WalletProjection = required(args, "public")?;
        let public = WalletPublic::from_projection(&projection);
        let secret: WalletSecret = ghost_storage::open_json(password, &sealed, "wallet vault")?;
        ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
        Ok(Self { profile_id, secret, public })
    }

    pub(in crate::native::browser_host) async fn send(&mut self, args: &Value, destination: &str, payloads: Vec<Vec<u8>>, pending: bool, pending_id: Option<String>) -> Result<MailboxSendResult, String> {
        ghost_kaspa::validate_destination(destination)?;
        let portal = profile_portal(&self.profile_id, &self.public, endpoint_override(args, "wrpcEndpoint")).await?;
        let mut transaction_id = String::new();
        for payload in payloads {
            let sent = ghost_kaspa::wallet::send_payload(&portal, &self.secret, &self.public, destination, 0, &payload).await?;
            transaction_id = sent.transaction_id;
            self.public = sent.public;
        }
        Ok(MailboxSendResult {
            transaction_id,
            fee_sompi: "0".into(),
            mailbox_output_sompi: "0".into(),
            public: self.public.projection(),
            pending_handshake: pending,
            pending_id,
        })
    }
}

pub(in crate::native::browser_host) fn frame(carrier: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    ghost_protocol::fragment(ghost_core::Id128::new_random(), carrier)
}

pub(in crate::native::browser_host) fn decode_payloads(values: &[String]) -> Result<Vec<Vec<u8>>, String> {
    if values.is_empty() || values.len() > ghost_core::MAX_FRAGMENTS { return Err("mailbox control payload count is invalid".into()); }
    values.iter().map(|value| hex::decode(value).map_err(|_| "mailbox payload is not valid hex".to_string())).collect()
}

pub(in crate::native::browser_host) fn stego(value: &str) -> Result<ghost_hydra::StegoProfile, String> {
    ghost_hydra::StegoProfile::parse_ui_label(value)
}

pub(in crate::native::browser_host) fn discard() -> ghost_api::HydraMailboxResult {
    ghost_api::HydraMailboxResult { discard: true, ..Default::default() }
}
