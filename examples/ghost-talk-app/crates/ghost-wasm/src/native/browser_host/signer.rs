use crate::model::{BroadcastResult, WalletProjection};
use ghost_kaspa::wallet::WalletPublic;
use serde_json::Value;

use super::{
    kaspa::{endpoint_override, profile_portal},
    support::util::{decimal, required, required_str, to_value},
};

pub(super) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "wallet_prepare_signer_send" => prepare(args).await,
        "wallet_broadcast_signer_send" => broadcast(args).await,
        _ => Err(format!("unknown browser signer command: {command}")),
    }
}

async fn prepare(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let destination = required_str(args, "destination")?;
    let amount = decimal(required_str(args, "amountSompi")?, "amount")?;
    let fee = decimal(required_str(args, "feeSompi")?, "fee")?;
    let endpoint = args
        .get("options")
        .and_then(|value| endpoint_override(value, "wrpcEndpoint"));
    let portal = profile_portal(profile_id, &public, endpoint).await?;
    let pskt = ghost_kaspa::wallet::prepare_signer_send(&portal, &public, destination, amount, fee)
        .await?;
    to_value(pskt)
}

async fn broadcast(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let signed_pskt = required_str(args, "signedPskt")?;
    let endpoint = args
        .get("options")
        .and_then(|value| endpoint_override(value, "wrpcEndpoint"));
    let portal = profile_portal(profile_id, &public, endpoint).await?;
    let result = ghost_kaspa::wallet::broadcast_signer_send(&portal, &public, signed_pskt).await?;
    to_value(BroadcastResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        public: result.public.projection(),
    })
}
