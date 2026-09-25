use super::{KaspaBroadcastResult, WalletPublic};

pub async fn prepare_signer_send(
    portal: &crate::PortalFacade,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
) -> Result<String, String> {
    crate::validate_destination(destination)?;
    portal
        .plan_send_with_payload(
            public.portal_wallet(),
            destination,
            amount_sompi,
            requested_fee_sompi,
            &[],
        )
        .await
}

pub async fn broadcast_signer_send(
    portal: &crate::PortalFacade,
    public: &WalletPublic,
    signed_pskt: &str,
) -> Result<KaspaBroadcastResult, String> {
    let analysis = portal.analyze(signed_pskt).await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err("KasKold-signed transaction failed current Kaspa fee/mass policy".into());
    }
    let transaction_id = portal.broadcast_signed_pskb(signed_pskt).await?;
    let mut public = public.clone();
    let _ = public.advance_change_if_available();
    Ok(KaspaBroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public,
        timings: None,
    })
}
