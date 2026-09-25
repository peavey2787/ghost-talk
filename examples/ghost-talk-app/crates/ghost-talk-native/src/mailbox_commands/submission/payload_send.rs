use super::super::{
    gateway_helpers::{send_payload_reuse_change_via_gateway, send_payload_via_gateway},
    monitor::wallet_events::wait_for_wallet_transaction_event,
    send_state::PortalFacade,
};
use ghost_kaspa::wallet::WalletPublic;

#[expect(
    clippy::too_many_arguments,
    reason = "explicit transaction submission boundary"
)]
pub(crate) async fn send_mailbox_payloads_via_gateway(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    profile_id: &str,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payloads: &[Vec<u8>],
    reuse_change: bool,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    validate_mailbox_payloads(payloads)?;
    if payloads.len() == 1 {
        return send_single_mailbox_payload(
            gateway,
            wallet_state,
            profile_id,
            portal,
            secret,
            public,
            destination,
            fee,
            &payloads[0],
            reuse_change,
        )
        .await;
    }
    send_fragmented_mailbox_payloads(
        gateway,
        wallet_state,
        profile_id,
        portal,
        secret,
        public,
        destination,
        fee,
        payloads,
        reuse_change,
    )
    .await
}

fn validate_mailbox_payloads(payloads: &[Vec<u8>]) -> Result<(), String> {
    if payloads.is_empty() || payloads.len() > ghost_core::MAX_FRAGMENTS {
        return Err("Ghost Talk logical carrier has an invalid physical fragment count".into());
    }
    if payloads
        .iter()
        .any(|payload| payload.is_empty() || payload.len() > ghost_core::MAX_KSPT_V1_PAYLOAD_BYTES)
    {
        return Err("Ghost Talk attempted to sign a payload larger than KSPT v1 permits".into());
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit transaction submission boundary"
)]
async fn send_single_mailbox_payload(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    profile_id: &str,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
    reuse_change: bool,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    let mut utxo_events = wallet_state.utxo_event_receiver(profile_id)?;
    let result = if reuse_change {
        send_payload_reuse_change_via_gateway(
            gateway,
            portal,
            secret,
            public,
            destination,
            fee,
            payload,
        )
        .await?
    } else {
        send_payload_via_gateway(gateway, portal, secret, public, destination, fee, payload).await?
    };
    log_observer_lag(
        profile_id,
        &result.transaction_id,
        "accepted-transaction-observer-lag",
        wait_for_wallet_transaction_event(&mut utxo_events, &result.transaction_id).await,
    );
    Ok(result)
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit transaction submission boundary"
)]
async fn send_fragmented_mailbox_payloads(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    profile_id: &str,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payloads: &[Vec<u8>],
    reuse_change: bool,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    let mut total_fee = 0u128;
    let mut last_transaction_id = String::new();
    for payload in payloads {
        let mut utxo_events = wallet_state.utxo_event_receiver(profile_id)?;
        let result = send_payload_reuse_change_via_gateway(
            gateway,
            portal,
            secret,
            public,
            destination,
            fee,
            payload,
        )
        .await?;
        total_fee = add_fragment_fee(total_fee, &result.fee_sompi)?;
        last_transaction_id = result.transaction_id;
        log_observer_lag(
            profile_id,
            &last_transaction_id,
            "accepted-fragment-observer-lag",
            wait_for_wallet_transaction_event(&mut utxo_events, &last_transaction_id).await,
        );
    }
    Ok(fragmented_result(
        public,
        reuse_change,
        last_transaction_id,
        total_fee,
    ))
}

fn add_fragment_fee(total: u128, fee_sompi: &str) -> Result<u128, String> {
    let fee = fee_sompi
        .parse::<u128>()
        .map_err(|_| "Portal returned a non-decimal fragment fee".to_string())?;
    total
        .checked_add(fee)
        .ok_or_else(|| "Ghost Talk aggregate mailbox fee overflowed u128".to_string())
}

fn fragmented_result(
    public: &WalletPublic,
    reuse_change: bool,
    transaction_id: String,
    total_fee: u128,
) -> ghost_kaspa::wallet::KaspaBroadcastResult {
    let mut next_public = public.clone();
    if !reuse_change {
        let _ = next_public.advance_change_if_available();
    }
    ghost_kaspa::wallet::KaspaBroadcastResult {
        transaction_id,
        fee_sompi: total_fee.to_string(),
        public: next_public,
        timings: None,
    }
}

fn log_observer_lag(
    profile_id: &str,
    transaction_id: &str,
    event: &str,
    observation: Result<(), String>,
) {
    if let Err(error) = observation {
        crate::debug_log::record(
            "warn",
            "kaspa",
            event,
            format!(
                "profile={} txid={} error={}",
                profile_id, transaction_id, error
            ),
        );
    }
}
