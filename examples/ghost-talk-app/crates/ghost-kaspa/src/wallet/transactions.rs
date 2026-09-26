use super::{
    account_key,
    planning::{plan_signed_send, PlannedSend, SendRequest},
    WalletPublic, WalletSecret, MAILBOX_OUTPUT_SOMPI,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastTimings {
    pub utxo_plan_ms: u64,
    pub signed_analysis_ms: u64,
    pub submit_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KaspaBroadcastResult {
    pub transaction_id: String,
    pub fee_sompi: String,
    pub public: WalletPublic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<BroadcastTimings>,
}

struct PreparedSend {
    signed: String,
    fee_sompi: String,
    utxo_plan_ms: u64,
    signed_analysis_ms: u64,
}

async fn prepare_send(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
) -> Result<PreparedSend, String> {
    let account = account_key(secret)?;
    let plan_started = Instant::now();
    let request = SendRequest {
        destination,
        amount_sompi,
        requested_fee_sompi,
        payload: &[],
    };
    let PlannedSend { signed, analysis } =
        plan_signed_send(portal, public, &account, request).await?;
    let utxo_plan_ms = plan_started.elapsed().as_millis() as u64;
    let analysis_started = Instant::now();
    Ok(PreparedSend {
        signed,
        fee_sompi: analysis.fee_sompi.to_string(),
        utxo_plan_ms,
        signed_analysis_ms: analysis_started.elapsed().as_millis() as u64,
    })
}

pub async fn send(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
) -> Result<KaspaBroadcastResult, String> {
    let fee = requested_fee_sompi;
    super::conflict::retry_on_spent_conflict(|| {
        send_once(portal, secret, public, destination, amount_sompi, fee)
    })
    .await
}

async fn send_once(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
) -> Result<KaspaBroadcastResult, String> {
    let total_started = Instant::now();
    crate::validate_destination(destination)?;
    let mut public = public.clone();
    let prepared = prepare_send(
        portal,
        secret,
        &public,
        destination,
        amount_sompi,
        requested_fee_sompi,
    )
    .await?;
    let submit_started = Instant::now();
    let transaction_id = portal.broadcast_signed_pskb(&prepared.signed).await?;
    let submit_ms = submit_started.elapsed().as_millis() as u64;
    let _ = public.advance_change_if_available();
    Ok(KaspaBroadcastResult {
        transaction_id,
        fee_sompi: prepared.fee_sompi,
        public,
        timings: Some(BroadcastTimings {
            utxo_plan_ms: prepared.utxo_plan_ms,
            signed_analysis_ms: prepared.signed_analysis_ms,
            submit_ms,
            total_ms: total_started.elapsed().as_millis() as u64,
        }),
    })
}

pub async fn send_payload(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
) -> Result<KaspaBroadcastResult, String> {
    send_payload_with_change_policy(
        portal,
        secret,
        public,
        destination,
        requested_fee_sompi,
        payload,
        true,
    )
    .await
}

/// High-frequency voice media deliberately reuses the current change address.
/// Rotating one HD change address per ~350 ms audio window would exhaust the
/// bounded lookahead during an ordinary call and continually restart frontend
/// monitoring. The caller still serializes broadcasts; ordinary wallet/message
/// sends retain fresh-change rotation through `send_payload`.
pub async fn send_payload_reuse_change(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
) -> Result<KaspaBroadcastResult, String> {
    send_payload_with_change_policy(
        portal,
        secret,
        public,
        destination,
        requested_fee_sompi,
        payload,
        false,
    )
    .await
}

pub(crate) async fn send_payload_with_change_policy(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
    advance_change: bool,
) -> Result<KaspaBroadcastResult, String> {
    crate::validate_destination(destination)?;
    if payload.is_empty() {
        return Err("mailbox payload must not be empty".into());
    }
    let (fee, change) = (requested_fee_sompi, advance_change);
    super::conflict::retry_on_spent_conflict(|| {
        send_payload_once(portal, secret, public, destination, fee, payload, change)
    })
    .await
}

async fn send_payload_once(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
    advance_change: bool,
) -> Result<KaspaBroadcastResult, String> {
    let account = account_key(secret)?;
    let mut public = public.clone();
    let request = SendRequest {
        destination,
        amount_sompi: MAILBOX_OUTPUT_SOMPI,
        requested_fee_sompi,
        payload,
    };
    let PlannedSend { signed, analysis } =
        plan_signed_send(portal, &public, &account, request).await?;
    let transaction_id = portal.broadcast_signed_pskb(&signed).await?;
    if advance_change {
        // Never turn a successful broadcast into an error merely because there is
        // no *next* prederived change address. Reuse the final monitored address.
        let _ = public.advance_change_if_available();
    }
    Ok(KaspaBroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public,
        timings: None,
    })
}
