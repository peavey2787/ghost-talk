use super::{account_key, sign_pskb, KaspaBroadcastResult, WalletPublic, WalletSecret};

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
