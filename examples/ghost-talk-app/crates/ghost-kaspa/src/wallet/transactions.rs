use super::{account_key, sign_pskb, WalletPublic, WalletSecret, MAILBOX_OUTPUT_SOMPI};
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
    let wire = portal
        .plan_send_with_payload(
            public.portal_wallet(),
            destination,
            amount_sompi,
            requested_fee_sompi,
            &[],
        )
        .await?;
    let utxo_plan_ms = plan_started.elapsed().as_millis() as u64;
    let analysis_started = Instant::now();
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal.analyze(&signed).await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err(
            "Portal 1.0.1 payload-aware planner produced a transaction that failed final fee/mass policy"
                .into(),
        );
    }
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
    let account = account_key(secret)?;
    let mut public = public.clone();
    let wire = portal
        .plan_send_with_payload(
            public.portal_wallet(),
            destination,
            MAILBOX_OUTPUT_SOMPI,
            requested_fee_sompi,
            payload,
        )
        .await?;
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal.analyze(&signed).await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err(
            "Portal 1.0.1 payload-aware planner produced a mailbox transaction that failed final fee/mass policy"
                .into(),
        );
    }
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

enum ConsolidationStep {
    Broadcast(KaspaBroadcastResult),
    Replan(u64),
}

async fn consolidation_step(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    fee_sompi: u64,
) -> Result<ConsolidationStep, String> {
    let account = account_key(secret)?;
    let wire = portal
        .plan_consolidation(public.portal_wallet(), fee_sompi)
        .await?;
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal.analyze(&signed).await?;
    if !analysis.mass_valid {
        return Err(format!(
            "Consolidation exceeds Kaspa mass policy (compute={}, transient={}, storage={})",
            analysis.compute_mass, analysis.transient_mass, analysis.storage_mass
        ));
    }
    if let Some(recommended) = consolidation_replan_fee(
        fee_sompi,
        analysis.fee_sufficient,
        analysis.minimum_fee_sompi,
        analysis.recommended_fee_sompi,
    )? {
        return Ok(ConsolidationStep::Replan(recommended));
    }
    let transaction_id = portal.broadcast_signed_pskb(&signed).await?;
    Ok(ConsolidationStep::Broadcast(KaspaBroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public: public.clone(),
        timings: None,
    }))
}

pub(super) fn consolidation_replan_fee(
    current_fee_sompi: u64,
    fee_sufficient: bool,
    minimum_fee_sompi: u64,
    recommended_fee_sompi: u64,
) -> Result<Option<u64>, String> {
    if fee_sufficient {
        return Ok(None);
    }
    let next = recommended_fee_sompi.max(minimum_fee_sompi);
    if next <= current_fee_sompi {
        return Err(format!(
            "Portal rejected consolidation fee policy even at {} sompi",
            current_fee_sompi
        ));
    }
    Ok(Some(next))
}

pub async fn consolidate(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    requested_fee_sompi: u64,
) -> Result<KaspaBroadcastResult, String> {
    const MAX_FEE_PASSES: usize = 3;
    let mut fee_sompi = requested_fee_sompi;
    for _ in 0..MAX_FEE_PASSES {
        match consolidation_step(portal, secret, public, fee_sompi).await? {
            ConsolidationStep::Broadcast(result) => return Ok(result),
            ConsolidationStep::Replan(recommended) => fee_sompi = recommended,
        }
    }
    Err("Consolidation fee planning did not converge on the current node policy".into())
}

#[cfg(test)]
mod fee_policy_tests {
    use super::consolidation_replan_fee;

    #[test]
    fn consolidation_replan_uses_toccata_floor() {
        assert_eq!(
            consolidation_replan_fee(100, false, 150, 125).unwrap(),
            Some(150)
        );
        assert_eq!(
            consolidation_replan_fee(100, false, 125, 175).unwrap(),
            Some(175)
        );
    }

    #[test]
    fn consolidation_replan_stops_when_fee_is_sufficient_or_cannot_advance() {
        assert_eq!(consolidation_replan_fee(100, true, 150, 175).unwrap(), None);
        assert!(consolidation_replan_fee(175, false, 150, 175).is_err());
    }
}
