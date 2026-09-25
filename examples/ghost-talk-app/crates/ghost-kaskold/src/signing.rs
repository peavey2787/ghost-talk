use ghost_api::{KasKoldReviewOutput, KasKoldReviewResult, KasKoldSignResult};
use sha2::{Digest, Sha256};

use crate::{open_runtime, runtime_error};

fn pskt_review_token(pskt_hex: &str, network: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(network.as_bytes());
    digest.update([0]);
    digest.update(pskt_hex.as_bytes());
    hex::encode(digest.finalize())
}

fn review_result(review: vault_runtime::VaultReview, token: String) -> KasKoldReviewResult {
    KasKoldReviewResult {
        review_token: token,
        network: review.network.into(),
        input_count: review.input_count,
        output_count: review.output_count,
        input_total_sompi: review.input_total.to_string(),
        output_total_sompi: review.output_total.to_string(),
        external_total_sompi: review.external_total.to_string(),
        change_total_sompi: review.change_total.to_string(),
        own_receive_total_sompi: review.own_receive_total.to_string(),
        fee_sompi: review.fee.to_string(),
        outputs: review.outputs.into_iter().map(|output| KasKoldReviewOutput {
            index: output.index,
            amount_sompi: output.amount.to_string(),
            ownership: output.ownership.into(),
            address: output.address,
        }).collect(),
    }
}

pub fn review_pskt(
    password: &str,
    sealed_inventory: &[u8],
    pskt_hex: &str,
    network: &str,
) -> Result<KasKoldReviewResult, String> {
    let mut runtime = open_runtime(password, sealed_inventory)?;
    let parsed_network = kaskold_sdk::Network::parse(network).map_err(|error| error.to_string())?;
    let request = kaskold_sdk::prepare(pskt_hex, parsed_network).map_err(|error| error.to_string())?;
    let wire = hex::decode(&request.kspt_hex).map_err(|error| format!("KasKold KSPT hex: {error}"))?;
    let review = runtime.load_transaction_file(&wire).map_err(runtime_error)?;
    Ok(review_result(review, pskt_review_token(pskt_hex, network)))
}

pub fn sign_pskt(
    password: &str,
    sealed_inventory: &[u8],
    pskt_hex: &str,
    network: &str,
    review_token: &str,
) -> Result<KasKoldSignResult, String> {
    if review_token != pskt_review_token(pskt_hex, network) {
        return Err("KasKold transaction changed after review; review it again before signing".into());
    }
    let mut runtime = open_runtime(password, sealed_inventory)?;
    let network = kaskold_sdk::Network::parse(network).map_err(|error| error.to_string())?;
    let request = kaskold_sdk::prepare(pskt_hex, network).map_err(|error| error.to_string())?;
    let wire = hex::decode(&request.kspt_hex).map_err(|error| format!("KasKold KSPT hex: {error}"))?;
    runtime.load_transaction_file(&wire).map_err(runtime_error)?;
    runtime.approve().map_err(runtime_error)?;
    let response_hex = hex::encode(runtime.signed_response_wire().map_err(runtime_error)?);
    let signed = kaskold_sdk::complete(&request, &response_hex).map_err(|error| error.to_string())?;
    let transaction_json = kaskold_sdk::finalize(&signed).map_err(|error| error.to_string())?;
    Ok(KasKoldSignResult { signed_pskt_hex: signed.pskt_hex, transaction_json })
}
