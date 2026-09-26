use super::{
    invoke, invoke_unit, json, BroadcastResult, Profile, WalletProjection, WalletRecovery,
};

pub async fn unlock_wallet(profile: &Profile, password: &str) -> Result<WalletProjection, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "wallet_unlock",
        json!({
            "profileId": profile.id, "password": password,
            "sealed": wallet.sealed, "public": wallet.public,
        }),
    )
    .await
}

pub async fn lock_profile(profile_id: &str) -> Result<(), String> {
    let monitor = invoke_unit("wallet_monitor_stop", json!({ "profileId": profile_id })).await;
    let wallet = invoke_unit("wallet_lock", json!({ "profileId": profile_id })).await;
    let hydra = invoke_unit("hydra_lock_profile", json!({ "profileId": profile_id })).await;
    hydra?;
    wallet?;
    monitor
}

pub async fn reveal_recovery(profile: &Profile, password: &str) -> Result<WalletRecovery, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "wallet_reveal_recovery",
        json!({
            "password": password, "sealed": wallet.sealed, "public": wallet.public,
        }),
    )
    .await
}

pub async fn gather_wallet_history(
    profile: &Profile,
    priority_addresses: &[String],
) -> Result<crate::model::WalletHistoryResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke("wallet_gather_history", json!({
        "public": wallet.public,
        "options": { "restEndpoint": wallet.rest_endpoint, "wrpcEndpoint": wallet.wrpc_endpoint },
        "priorityAddresses": priority_addresses,
    })).await
}

pub async fn send_kaspa(
    profile: &Profile,
    password: &str,
    destination: &str,
    amount_sompi: &str,
    fee_sompi: &str,
    reuse_unlocked: bool,
) -> Result<BroadcastResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke("wallet_send", json!({
        "profileId": profile.id, "reuseUnlocked": reuse_unlocked, "password": password,
        "sealed": wallet.sealed, "public": wallet.public, "destination": destination,
        "amountSompi": amount_sompi, "feeSompi": fee_sompi,
        "options": { "restEndpoint": wallet.rest_endpoint, "wrpcEndpoint": wallet.wrpc_endpoint },
    })).await
}

pub async fn prepare_signer_send(
    profile: &Profile,
    destination: &str,
    amount_sompi: &str,
    fee_sompi: &str,
) -> Result<String, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke("wallet_prepare_signer_send", json!({
        "profileId": profile.id,
        "public": wallet.public,
        "destination": destination,
        "amountSompi": amount_sompi,
        "feeSompi": fee_sompi,
        "options": { "restEndpoint": wallet.rest_endpoint, "wrpcEndpoint": wallet.wrpc_endpoint },
    })).await
}

pub async fn broadcast_signer_send(
    profile: &Profile,
    signed_pskt: &str,
) -> Result<BroadcastResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke("wallet_broadcast_signer_send", json!({
        "profileId": profile.id,
        "public": wallet.public,
        "signedPskt": signed_pskt,
        "options": { "restEndpoint": wallet.rest_endpoint, "wrpcEndpoint": wallet.wrpc_endpoint },
    })).await
}

pub async fn consolidate_kaspa(
    profile: &Profile,
    password: &str,
    fee_sompi: &str,
    reuse_unlocked: bool,
) -> Result<BroadcastResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke("wallet_consolidate", json!({
        "profileId": profile.id, "reuseUnlocked": reuse_unlocked, "password": password,
        "sealed": wallet.sealed, "public": wallet.public, "feeSompi": fee_sompi,
        "options": { "restEndpoint": wallet.rest_endpoint, "wrpcEndpoint": wallet.wrpc_endpoint },
    })).await
}
