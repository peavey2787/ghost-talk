mod local;
pub(crate) use local::media_import_local;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_api::VerifiedMedia;
use ghost_media::{verify_content, MediaLocation, MediaReference};

const MAX_MEDIA_FETCH_BYTES: usize = 512 * 1024 * 1024;

#[tauri::command]
pub async fn media_fetch_verified(
    app: tauri::AppHandle,
    reference: MediaReference,
) -> Result<VerifiedMedia, String> {
    let bytes = match &reference.location {
        MediaLocation::Remote(url) => fetch_remote(url).await?,
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

fn validate_bytes(reference: &MediaReference, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_MEDIA_FETCH_BYTES || bytes.len() as u64 != reference.size {
        return Err("media size does not match the signed reference".into());
    }
    if !verify_content(bytes, &reference.content_hash) {
        return Err("media content does not match the signed reference".into());
    }
    Ok(())
}
