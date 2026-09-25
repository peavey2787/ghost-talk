mod archive;
mod archive_cache;
mod local;
mod manifest;
pub(crate) use archive::{kaspa_archive_plan, kaspa_archive_publish};
pub(crate) use local::media_import_local;
pub(crate) use local::store_bytes as store_local_media;
pub(crate) use manifest::media_manifest_sign;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_api::VerifiedMedia;
use ghost_media::{
    decode_archive_chunk, verify_content, KaspaArchiveLocator, MediaLocation, MediaReference,
};

const MAX_MEDIA_FETCH_BYTES: usize = 512 * 1024 * 1024;

#[tauri::command]
pub async fn media_fetch_verified(
    app: tauri::AppHandle,
    reference: MediaReference,
) -> Result<VerifiedMedia, String> {
    let bytes = match &reference.location {
        MediaLocation::Remote(url) => fetch_remote(url).await?,
        MediaLocation::KaspaArchive(locator) => {
            fetch_archive_cached(&app, &reference, locator).await?
        }
        MediaLocation::Local => local::load_bytes(&app, &reference.media_id)?,
    };
    validate_bytes(&reference, &bytes)?;
    Ok(VerifiedMedia {
        content_type: reference.content_type,
        data_base64: STANDARD.encode(bytes),
    })
}

async fn fetch_remote(url: &str) -> Result<Vec<u8>, String> {
    if !(url.starts_with("https://")
        || url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost"))
    {
        return Err("remote media must use HTTPS except for localhost development".into());
    }
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("media retrieval failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "media retrieval returned HTTP {}",
            response.status()
        ));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("media body read failed: {error}"))
}

async fn fetch_archive_cached(
    app: &tauri::AppHandle,
    reference: &MediaReference,
    encoded: &str,
) -> Result<Vec<u8>, String> {
    if let Some(bytes) = local::load_if_present(app, &reference.media_id)? {
        if validate_bytes(reference, &bytes).is_ok() {
            return Ok(bytes);
        }
        local::remove_if_present(app, &reference.media_id)?;
    }
    let bytes = fetch_kaspa_archive(app, reference, encoded).await?;
    validate_bytes(reference, &bytes)?;
    let _ = local::store_bytes(app, &reference.content_type, &bytes)?;
    Ok(bytes)
}

async fn fetch_kaspa_archive(
    app: &tauri::AppHandle,
    reference: &MediaReference,
    encoded: &str,
) -> Result<Vec<u8>, String> {
    let locator = KaspaArchiveLocator::decode(encoded)?;
    let mut chunks = locator.chunks.clone();
    validate_locator(reference, &chunks)?;
    chunks.sort_by_key(|chunk| chunk.index);
    let mut bodies = vec![None; chunks.len()];
    for (position, chunk) in chunks.iter().enumerate() {
        bodies[position] = archive_cache::load_chunk(app, chunk)?;
    }
    if bodies.iter().any(Option::is_none) {
        fill_missing_chunks(app, reference, &locator, &chunks, &mut bodies).await?;
    }
    bodies
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "Kaspa archive reconstruction is incomplete".into())
        .map(|parts| parts.into_iter().flatten().collect())
}

async fn fill_missing_chunks(
    app: &tauri::AppHandle,
    reference: &MediaReference,
    locator: &KaspaArchiveLocator,
    chunks: &[ghost_media::KaspaArchiveChunk],
    bodies: &mut [Option<Vec<u8>>],
) -> Result<(), String> {
    let rest =
        ghost_history::RestHistory::new(ghost_history::rest_base_for_network(&locator.network), 64);
    let history = rest
        .wallet_history(std::slice::from_ref(&locator.address))
        .await?;
    let by_id = history
        .transactions
        .into_iter()
        .map(|tx| (tx.transaction_id, tx.payload))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (position, expected) in chunks.iter().enumerate() {
        if bodies[position].is_some() {
            continue;
        }
        let payload = by_id.get(&expected.transaction_id).ok_or_else(|| {
            format!(
                "archive transaction {} is unavailable",
                expected.transaction_id
            )
        })?;
        let (media_id, index, count, bytes) = decode_archive_chunk(payload)?;
        let verified = media_id == reference.media_id
            && index == expected.index
            && count as usize == chunks.len()
            && bytes.len() == expected.size as usize
            && verify_content(&bytes, &expected.content_hash);
        if !verified {
            return Err("Kaspa archive chunk verification failed".into());
        }
        archive_cache::store_chunk(app, expected, &bytes)?;
        bodies[position] = Some(bytes);
    }
    Ok(())
}

fn validate_locator(
    reference: &MediaReference,
    chunks: &[ghost_media::KaspaArchiveChunk],
) -> Result<(), String> {
    if chunks.is_empty() {
        return Err("Kaspa archive locator has no chunks".into());
    }
    let mut ordered = chunks.to_vec();
    ordered.sort_by_key(|chunk| chunk.index);
    let mut txids = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for (position, chunk) in ordered.iter().enumerate() {
        total = total.saturating_add(validate_locator_chunk(position, chunk, &mut txids)?);
    }
    if total != reference.size {
        return Err("Kaspa archive locator size does not match the signed reference".into());
    }
    Ok(())
}

fn validate_locator_chunk<'a>(
    position: usize,
    chunk: &'a ghost_media::KaspaArchiveChunk,
    txids: &mut std::collections::BTreeSet<&'a str>,
) -> Result<u64, String> {
    if chunk.index != position as u32
        || chunk.size == 0
        || chunk.content_hash.len() != 64
        || !chunk
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || chunk.transaction_id.trim().is_empty()
        || !txids.insert(chunk.transaction_id.as_str())
    {
        return Err("Kaspa archive locator is inconsistent".into());
    }
    Ok(u64::from(chunk.size))
}

fn validate_bytes(reference: &MediaReference, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_MEDIA_FETCH_BYTES || bytes.len() as u64 != reference.size {
        return Err("media size does not match the signed reference".into());
    }
    if !verify_content(bytes, &reference.content_hash) {
        return Err("media content does not match the signed reference".into());
    }
    Ok(())
}
