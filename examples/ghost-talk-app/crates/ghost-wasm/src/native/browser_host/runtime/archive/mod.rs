mod progress;
mod publish;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_api::{KaspaArchivePlan, KaspaArchivePublishResult};
use ghost_kaspa::{validate_archive_size, wallet::WalletPublic, ArchivePlanRequest, ArchivePublishRequest};
use ghost_media::{
    decode_archive_chunk, verify_content, KaspaArchiveLocator, MediaReference,
};
use serde_json::Value;

use super::super::{kaspa::profile_portal, support::util::{open_wallet_secret, required, to_value}};

pub(in crate::native::browser_host) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "kaspa_archive_plan" => archive_plan(args).await,
        "kaspa_archive_publish" => archive_publish(args).await,
        _ => Err(format!("unknown browser Kaspa archive command: {command}")),
    }
}

async fn archive_plan(args: &Value) -> Result<Value, String> {
    let request: ArchivePlanRequest = required(args, "request")?;
    validate_archive_size(request.data_size)?;
    let secret = open_wallet_secret(&request.password, &request.sealed, &request.public)?;
    let address = stable_address(&request.public)?;
    let portal = profile_portal(&request.profile_id, &request.public, request.wrpc_endpoint.as_deref()).await?;
    let result: KaspaArchivePlan = ghost_kaspa::archive_plan(
        &portal, &secret, &request.public, &address, request.data_size,
    )
    .await?;
    to_value(result)
}

async fn archive_publish(args: &Value) -> Result<Value, String> {
    let request: ArchivePublishRequest = required(args, "request")?;
    if !request.confirmed {
        return Err("permanent Kaspa archive requires explicit confirmation".into());
    }
    let bytes = STANDARD.decode(&request.data_base64)
        .map_err(|error| format!("archive base64 failed: {error}"))?;
    validate_archive_size(bytes.len() as u64)?;
    let secret = open_wallet_secret(&request.password, &request.sealed, &request.public)?;
    let address = stable_address(&request.public)?;
    let portal = profile_portal(&request.profile_id, &request.public, request.wrpc_endpoint.as_deref()).await?;
    let result: KaspaArchivePublishResult = publish::archive(&portal, &secret, &request, &address, bytes).await?;
    to_value(result)
}

pub(in crate::native::browser_host) async fn fetch(reference: &MediaReference, encoded: &str) -> Result<Vec<u8>, String> {
    let locator = KaspaArchiveLocator::decode(encoded)?;
    validate_locator(reference, &locator)?;
    let payloads = super::history::raw_payloads(&locator.network, &locator.address).await?;
    let mut chunks = locator.chunks.clone();
    chunks.sort_by_key(|chunk| chunk.index);
    let mut out = Vec::with_capacity(reference.size as usize);
    for expected in &chunks {
        let payload = payloads.get(&expected.transaction_id)
            .ok_or_else(|| format!("archive transaction {} is unavailable", expected.transaction_id))?;
        let (media_id, index, count, body) = decode_archive_chunk(payload)?;
        let valid = media_id == reference.media_id
            && index == expected.index
            && count as usize == chunks.len()
            && body.len() == expected.size as usize
            && verify_content(&body, &expected.content_hash);
        if !valid { return Err("Kaspa archive chunk verification failed".into()); }
        out.extend_from_slice(&body);
    }
    if out.len() as u64 != reference.size || !verify_content(&out, &reference.content_hash) {
        return Err("Kaspa archive reconstruction failed final content verification".into());
    }
    Ok(out)
}

fn stable_address(public: &WalletPublic) -> Result<String, String> {
    public.receive_addresses.first().cloned().ok_or_else(|| "wallet has no stable archive address".into())
}

fn validate_locator(reference: &MediaReference, locator: &KaspaArchiveLocator) -> Result<(), String> {
    if locator.chunks.is_empty() { return Err("Kaspa archive locator has no chunks".into()); }
    let mut chunks = locator.chunks.clone();
    chunks.sort_by_key(|chunk| chunk.index);
    let mut ids = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for (position, chunk) in chunks.iter().enumerate() {
        let valid = chunk.index == position as u32 && chunk.size > 0
            && chunk.content_hash.len() == 64
            && chunk.content_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !chunk.transaction_id.trim().is_empty() && ids.insert(chunk.transaction_id.as_str());
        if !valid { return Err("Kaspa archive locator is inconsistent".into()); }
        total = total.saturating_add(u64::from(chunk.size));
    }
    if total != reference.size { return Err("Kaspa archive locator size does not match the signed reference".into()); }
    Ok(())
}
