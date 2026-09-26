//! Plan, sign and fee-check one send, replanning when the node's fee policy
//! asks for more than Portal's first estimate (e.g. a wallet fragmented into
//! many small UTXOs after heavy messaging).

use super::{consolidation::consolidation_replan_fee, sign_pskb, WalletPublic};
use crate::{PortalFacade, PortalMassAnalysis};
use kaspa_portal::wallet::derivation::bip32::ExtendedPrivKey;

const MAX_FEE_PASSES: usize = 3;

pub(super) struct PlannedSend {
    pub(super) signed: String,
    pub(super) analysis: PortalMassAnalysis,
}

pub(super) struct SendRequest<'a> {
    pub(super) destination: &'a str,
    pub(super) amount_sompi: u64,
    pub(super) requested_fee_sompi: u64,
    pub(super) payload: &'a [u8],
}

pub(super) async fn plan_signed_send(
    portal: &PortalFacade,
    public: &WalletPublic,
    account: &ExtendedPrivKey,
    request: SendRequest<'_>,
) -> Result<PlannedSend, String> {
    let mut fee_sompi = request.requested_fee_sompi;
    for _ in 0..MAX_FEE_PASSES {
        match plan_pass(portal, public, account, &request, fee_sompi).await? {
            Ok(planned) => return Ok(planned),
            Err(next_fee) => fee_sompi = next_fee,
        }
    }
    Err("Kaspa fee planning did not converge on the current node policy".into())
}

/// One plan/sign/analyze pass: the finished send, or the fee to replan with.
async fn plan_pass(
    portal: &PortalFacade,
    public: &WalletPublic,
    account: &ExtendedPrivKey,
    request: &SendRequest<'_>,
    fee_sompi: u64,
) -> Result<Result<PlannedSend, u64>, String> {
    let wire = portal
        .plan_send_with_payload(
            public.portal_wallet(),
            request.destination,
            request.amount_sompi,
            fee_sompi,
            request.payload,
        )
        .await?;
    let signed = sign_pskb(&wire, &public.network, account)?;
    let analysis = portal.analyze(&signed).await?;
    Ok(match fee_verdict(fee_sompi, &analysis)? {
        None => Ok(PlannedSend { signed, analysis }),
        Some(next_fee) => Err(next_fee),
    })
}

/// `None` when the analyzed transaction is acceptable, else the fee to retry.
fn fee_verdict(fee_sompi: u64, analysis: &PortalMassAnalysis) -> Result<Option<u64>, String> {
    if !analysis.mass_valid {
        return Err(mass_error(analysis));
    }
    consolidation_replan_fee(
        fee_sompi,
        analysis.fee_sufficient,
        analysis.minimum_fee_sompi,
        analysis.recommended_fee_sompi,
    )
}

fn mass_error(analysis: &PortalMassAnalysis) -> String {
    format!(
        "Kaspa transaction exceeds mass policy (compute={}, storage={}); consolidate the wallet and retry",
        analysis.compute_mass, analysis.storage_mass
    )
}
