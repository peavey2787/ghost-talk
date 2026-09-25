use super::{account_key, sign_pskb, KaspaBroadcastResult, WalletPublic, WalletSecret};

pub async fn send_payload_amount(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
    payload: &[u8],
) -> Result<KaspaBroadcastResult, String> {
    crate::validate_destination(destination)?;
    if payload.is_empty() {
        return Err("transaction payload must not be empty".into());
    }
    let account = account_key(secret)?;
    let mut public = public.clone();
    let wire = portal
        .plan_send_with_payload(
            public.portal_wallet(),
            destination,
            amount_sompi,
            requested_fee_sompi,
            payload,
        )
        .await?;
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal.analyze(&signed).await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err("payload transaction failed final fee/mass policy".into());
    }
    let transaction_id = portal.broadcast_signed_pskb(&signed).await?;
    let _ = public.advance_change_if_available();
    Ok(KaspaBroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public,
        timings: None,
    })
}

pub async fn estimate_payload_fee(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    payload: &[u8],
) -> Result<u64, String> {
    crate::validate_destination(destination)?;
    if payload.is_empty() {
        return Err("transaction payload must not be empty".into());
    }
    let account = account_key(secret)?;
    let wire = portal
        .plan_send_with_payload(public.portal_wallet(), destination, 0, 0, payload)
        .await?;
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal.analyze(&signed).await?;
    if !analysis.mass_valid {
        return Err("payload transaction exceeds current Kaspa mass policy".into());
    }
    Ok(analysis
        .recommended_fee_sompi
        .max(analysis.minimum_fee_sompi))
}
