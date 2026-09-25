use super::super::send_state::{
    LiveBlockStream, LiveTransactionObservation, MailboxEvent, PortalFacade,
};
use ghost_kaspa::wallet::WalletPublic;
use std::time::Duration;
pub(crate) const STALE_SPEND_RETRY_ATTEMPTS: usize = 20;
pub(crate) const STALE_SPEND_RETRY_DELAY: Duration = Duration::from_millis(250);

pub(crate) fn retryable_stale_spend_rejection(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("rejected transaction")
        && normalized.contains("already spent by transaction")
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit transaction submission boundary"
)]
pub(crate) async fn send_payload_with_stale_spend_retry(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
    reuse_change: bool,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    for attempt in 1..=STALE_SPEND_RETRY_ATTEMPTS {
        let outcome = submit_payload_attempt(
            gateway,
            portal,
            secret,
            public,
            destination,
            fee,
            payload,
            reuse_change,
            attempt,
        )
        .await;
        let Some(result) = outcome else {
            continue;
        };
        return result;
    }
    unreachable!("bounded stale-spend retry loop always returns")
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit transaction submission boundary"
)]
async fn submit_payload_attempt(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
    reuse_change: bool,
    attempt: usize,
) -> Option<Result<ghost_kaspa::wallet::KaspaBroadcastResult, String>> {
    let error = match send_payload_attempt(
        portal,
        secret,
        public,
        destination,
        fee,
        payload,
        reuse_change,
    )
    .await
    {
        Ok(result) => return Some(Ok(result)),
        Err(error) => error,
    };
    if should_retry_stale_spend(&error, attempt) {
        log_stale_spend_retry(attempt, &error);
        tokio::time::sleep(STALE_SPEND_RETRY_DELAY).await;
        None
    } else {
        gateway.note_operation_error(&error).await;
        Some(Err(error))
    }
}

async fn send_payload_attempt(
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
    reuse_change: bool,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    if reuse_change {
        ghost_kaspa::wallet::send_payload_reuse_change(
            portal,
            secret,
            public,
            destination,
            fee,
            payload,
        )
        .await
    } else {
        ghost_kaspa::wallet::send_payload(portal, secret, public, destination, fee, payload).await
    }
}

fn should_retry_stale_spend(error: &str, attempt: usize) -> bool {
    attempt < STALE_SPEND_RETRY_ATTEMPTS && retryable_stale_spend_rejection(error)
}

fn log_stale_spend_retry(attempt: usize, error: &str) {
    // SubmitTransaction explicitly rejected this attempt, so this is not an
    // ambiguous broadcast. Re-plan only after the same node's UTXO view catches up.
    crate::debug_log::record(
        "warn",
        "kaspa",
        "stale-spend-replan",
        format!(
            "attempt={} max_attempts={} retry_ms={} error={}",
            attempt,
            STALE_SPEND_RETRY_ATTEMPTS,
            STALE_SPEND_RETRY_DELAY.as_millis(),
            error
        ),
    );
}

pub(crate) async fn send_payload_reuse_change_via_gateway(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    send_payload_with_stale_spend_retry(
        gateway,
        portal,
        secret,
        public,
        destination,
        fee,
        payload,
        true,
    )
    .await
}

pub(crate) async fn send_payload_via_gateway(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    send_payload_with_stale_spend_retry(
        gateway,
        portal,
        secret,
        public,
        destination,
        fee,
        payload,
        false,
    )
    .await
}

pub(crate) async fn start_live_block_stream(
    profile_id: &str,
    public: &WalletPublic,
    wrpc_override: Option<&str>,
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
) -> Result<LiveBlockStream, String> {
    let blocks = gateway.subscribe_blocks(public, wrpc_override).await?;
    crate::debug_log::record(
        "info",
        "mailbox",
        "block-added-subscribed",
        format!(
            "profile={} network={} gateway_fanout=true portal_rpc_plus_block_stream=true",
            profile_id, public.network
        ),
    );
    Ok(LiveBlockStream { blocks })
}

pub(crate) fn live_public_profile_updates(
    observations: &[LiveTransactionObservation],
    tip: u64,
) -> Vec<crate::peer_commands::PublicGhostProfile> {
    let mut latest =
        std::collections::BTreeMap::<String, (u64, crate::peer_commands::PublicGhostProfile)>::new(
        );
    for observation in observations {
        let Some(profile) = verified_public_profile(observation, tip) else {
            continue;
        };
        remember_newest_public_profile(&mut latest, observation.daa_score, profile);
    }
    latest.into_values().map(|(_, profile)| profile).collect()
}

pub(crate) fn verified_public_profile(
    observation: &LiveTransactionObservation,
    tip: u64,
) -> Option<crate::peer_commands::PublicGhostProfile> {
    if !observation.payload.starts_with(&ghost_protocol::GTCD_MAGIC) {
        return None;
    }
    let descriptor = ghost_protocol::GhostContactDescriptor::decode(&observation.payload).ok()?;
    if !valid_live_descriptor(&descriptor, tip) {
        return None;
    }
    Some(crate::peer_commands::PublicGhostProfile {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_identity_id: descriptor.hydra_identity_id,
        descriptor_blue_score: observation.daa_score.to_string(),
        kns_name: None,
        dotk_name: None,
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
        capabilities: descriptor.capabilities,
        signature: descriptor.signature_hex,
        verified: true,
        discoverable: descriptor.discoverable,
        ..Default::default()
    })
}

pub(crate) fn valid_live_descriptor(
    descriptor: &ghost_protocol::GhostContactDescriptor,
    tip: u64,
) -> bool {
    descriptor.version == 1
        && descriptor.hydra_identity_id.len() == 64
        && descriptor
            .hydra_identity_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        && !descriptor
            .expires_daa
            .is_some_and(|expires| tip != 0 && expires <= tip)
        && ghost_kaspa::verify_gtcd(descriptor).is_ok()
}

pub(crate) fn remember_newest_public_profile(
    latest: &mut std::collections::BTreeMap<
        String,
        (u64, crate::peer_commands::PublicGhostProfile),
    >,
    score: u64,
    profile: crate::peer_commands::PublicGhostProfile,
) {
    let address = profile.kaspa_address.clone();
    if latest
        .get(&address)
        .is_some_and(|(existing, _)| *existing > score)
    {
        return;
    }
    latest.insert(address, (score, profile));
}

pub(crate) fn to_live_mailbox_event(observation: LiveTransactionObservation) -> MailboxEvent {
    MailboxEvent {
        transaction_id: observation.txid,
        blue_score: observation.daa_score.to_string(),
        payload_hex: hex::encode(observation.payload),
        block_time: None,
    }
}

#[cfg(test)]
mod gateway_helper_tests {
    use super::*;

    #[test]
    fn retries_only_explicit_already_spent_rejections() {
        assert!(retryable_stale_spend_rejection(
            "RPC error: Rejected transaction deadbeef: output (abc, 1) already spent by transaction 6"
        ));
        assert!(!retryable_stale_spend_rejection(
            "WebSocket RPC response timeout (15s)"
        ));
        assert!(!retryable_stale_spend_rejection("insufficient funds"));
    }
}
