use ghost_api::{MailboxSendResult, SwarmDelivery};
use ghost_domain::settings::SpendMode;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};

use crate::native::browser_host::{
    kaspa::{endpoint_override, profile_portal, remove_portal},
    support::util::{required, required_str},
};

thread_local! {
    static SPEND_MODES: RefCell<HashMap<String, SpendMode>> = RefCell::new(HashMap::new());
}

/// Record whether this profile's mailbox sends spend Kaspa or go to the swarm.
pub(in crate::native::browser_host) fn set_spend_mode(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?.to_owned();
    let mode: SpendMode = required(args, "spendMode")?;
    SPEND_MODES.with(|modes| modes.borrow_mut().insert(profile_id, mode));
    Ok(Value::Null)
}

fn spend_mode(profile_id: &str) -> SpendMode {
    SPEND_MODES.with(|modes| modes.borrow().get(profile_id).copied().unwrap_or_default())
}

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
        Ok(Self {
            profile_id,
            secret,
            public,
        })
    }

    pub(in crate::native::browser_host) async fn send(
        &mut self,
        args: &Value,
        destination: &str,
        payloads: Vec<Vec<u8>>,
        pending: bool,
        pending_id: Option<String>,
    ) -> Result<MailboxSendResult, String> {
        ghost_kaspa::validate_destination(destination)?;
        if spend_mode(&self.profile_id) == SpendMode::NoSpend {
            return Ok(swarm_send_result(
                &self.public,
                destination,
                &payloads,
                pending,
                pending_id,
            ));
        }
        let endpoint = endpoint_override(args, "wrpcEndpoint");
        let mut portal = profile_portal(&self.profile_id, &self.public, endpoint).await?;
        let mut transaction_id = String::new();
        for payload in payloads {
            let sent = self
                .send_with_retry(&mut portal, endpoint, destination, &payload)
                .await?;
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
            swarm: None,
        })
    }
}

/// Attempts per payload. Every attempt spends the same UTXOs, so at most one
/// transaction can ever confirm and a retry never duplicates the message.
const SEND_ATTEMPTS: usize = 3;
/// Wait before resubmitting a transaction whose parent the node lacks.
const ORPHAN_RETRY_MS: u32 = 3_000;

impl BrowserWallet {
    /// Send one payload, retrying transient failures: a dropped node connection
    /// reconnects, and an orphan rejection waits for its parent to propagate.
    async fn send_with_retry(
        &self,
        portal: &mut ghost_kaspa::PortalFacade,
        endpoint: Option<&str>,
        destination: &str,
        payload: &[u8],
    ) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
        let mut attempt = 1;
        loop {
            let error = match self.send_payload(portal, destination, payload).await {
                Ok(sent) => return Ok(sent),
                Err(error) if attempt == SEND_ATTEMPTS => return Err(error),
                Err(error) => error,
            };
            self.recover(portal, endpoint, error).await?;
            attempt += 1;
        }
    }

    /// Prepare the next attempt after a transient failure; any other error
    /// is returned as final.
    async fn recover(
        &self,
        portal: &mut ghost_kaspa::PortalFacade,
        endpoint: Option<&str>,
        error: String,
    ) -> Result<(), String> {
        if ghost_kaspa::is_portal_transport_failure(&error) {
            remove_portal(&self.profile_id);
            *portal = profile_portal(&self.profile_id, &self.public, endpoint).await?;
            return Ok(());
        }
        if ghost_kaspa::is_orphan_rejection(&error) {
            gloo_timers::future::TimeoutFuture::new(ORPHAN_RETRY_MS).await;
            return Ok(());
        }
        Err(error)
    }

    async fn send_payload(
        &self,
        portal: &ghost_kaspa::PortalFacade,
        destination: &str,
        payload: &[u8],
    ) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
        ghost_kaspa::wallet::send_payload(
            portal,
            &self.secret,
            &self.public,
            destination,
            0,
            payload,
        )
        .await
    }
}

/// A no-spend send: nothing is signed into a Kaspa transaction; the caller
/// publishes the exact frames on the swarm mailbox. The id of the last frame
/// stands in for the transaction id, as the last transaction's id does for a
/// fragmented Kaspa send.
fn swarm_send_result(
    public: &WalletPublic,
    destination: &str,
    payloads: &[Vec<u8>],
    pending: bool,
    pending_id: Option<String>,
) -> MailboxSendResult {
    MailboxSendResult {
        transaction_id: payloads
            .last()
            .map(|frame| ghost_protocol::swarm_frame_id(frame))
            .unwrap_or_default(),
        fee_sompi: "0".into(),
        mailbox_output_sompi: "0".into(),
        public: public.projection(),
        pending_handshake: pending,
        pending_id,
        swarm: Some(SwarmDelivery {
            destination: destination.to_owned(),
            frames_hex: payloads.iter().map(hex::encode).collect(),
        }),
    }
}

pub(in crate::native::browser_host) fn frame(carrier: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    ghost_protocol::fragment(ghost_core::Id128::new_random(), carrier)
}

pub(in crate::native::browser_host) fn decode_payloads(
    values: &[String],
) -> Result<Vec<Vec<u8>>, String> {
    if values.is_empty() || values.len() > ghost_core::MAX_FRAGMENTS {
        return Err("mailbox control payload count is invalid".into());
    }
    values
        .iter()
        .map(|value| hex::decode(value).map_err(|_| "mailbox payload is not valid hex".to_string()))
        .collect()
}

pub(in crate::native::browser_host) fn stego(
    value: &str,
) -> Result<ghost_hydra::StegoProfile, String> {
    ghost_hydra::StegoProfile::parse_ui_label(value)
}

pub(in crate::native::browser_host) fn discard() -> ghost_api::HydraMailboxResult {
    ghost_api::HydraMailboxResult {
        discard: true,
        ..Default::default()
    }
}
