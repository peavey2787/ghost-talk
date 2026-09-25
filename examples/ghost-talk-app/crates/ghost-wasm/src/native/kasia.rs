use crate::model::Profile;
use ghost_api::BroadcastResult;
use ghost_kasia::{KasiaContactMapping, KasiaMessage, KasiaReceivedHandshake};
use serde_json::json;

fn wallet_request(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
) -> Result<serde_json::Value, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    Ok(json!({
        "profileId": profile.id,
        "password": password,
        "sealed": wallet.sealed,
        "public": wallet.public,
        "mapping": mapping,
        "feeSompi": 0u64,
        "wrpcEndpoint": wallet.wrpc_endpoint,
    }))
}

pub(crate) async fn send_kasia_handshake(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    is_response: bool,
) -> Result<BroadcastResult, String> {
    let mut request = wallet_request(profile, password, mapping)?;
    request["isResponse"] = json!(is_response);
    super::invoke::invoke("kasia_send_handshake", json!({ "request": request })).await
}

pub(crate) async fn send_kasia_message(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    text: &str,
) -> Result<BroadcastResult, String> {
    let mut request = wallet_request(profile, password, mapping)?;
    request["text"] = json!(text);
    super::invoke::invoke("kasia_send_message", json!({ "request": request })).await
}

pub(crate) async fn kasia_history(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    indexer_url: &str,
    block_time: u64,
) -> Result<Vec<KasiaMessage>, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    super::invoke::invoke(
        "kasia_history",
        json!({
            "request": {
                "profileId": profile.id,
                "password": password,
                "sealed": wallet.sealed,
                "public": wallet.public,
                "mapping": mapping,
                "indexerUrl": indexer_url,
                "blockTime": block_time,
            }
        }),
    )
    .await
}

pub(crate) async fn received_kasia_handshakes(
    profile: &Profile,
    password: &str,
    indexer_url: &str,
    block_time: u64,
) -> Result<Vec<KasiaReceivedHandshake>, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    super::invoke::invoke(
        "kasia_received_handshakes",
        json!({
            "profileId": profile.id,
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "indexerUrl": indexer_url,
            "blockTime": block_time,
        }),
    )
    .await
}
