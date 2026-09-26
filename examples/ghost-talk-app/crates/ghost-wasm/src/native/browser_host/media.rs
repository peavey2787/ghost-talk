use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_api::VerifiedMedia;
use ghost_media::{
    content_hash, verify_content, GhostMediaManifest, MediaLocation, MediaReference,
};
use js_sys::Uint8Array;
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

use super::support::util::{required, required_str, to_value};

const MEDIA_KEY_PREFIX: &str = "ghost-talk.media.v1.";
const MAX_BROWSER_MEDIA_BYTES: usize = ghost_kaspa::MAX_ARCHIVE_BYTES;
const MAX_PERSISTED_ASSET_BYTES: usize = 4 * 1024 * 1024;

thread_local! {
    static MEDIA_CACHE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

pub(super) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "media_import_local" => import_local(args),
        "media_fetch_verified" => fetch_verified(args).await,
        "media_manifest_sign" => sign_manifest(args),
        "kaspa_archive_plan" | "kaspa_archive_publish" => {
            super::runtime::archive::invoke(command, args).await
        }
        _ => Err(format!("unknown browser media command: {command}")),
    }
}

fn sign_manifest(args: &Value) -> Result<Value, String> {
    const DOMAIN: &[u8] = b"GhostMediaManifest/v1";
    let request = args
        .get("request")
        .ok_or("browser command argument request is missing")?;
    let password = required_str(request, "password")?;
    let sealed: Vec<u8> = required(request, "sealed")?;
    let projection: crate::model::WalletProjection = required(request, "public")?;
    let public = ghost_kaspa::wallet::WalletPublic::from_projection(&projection);
    let secret: ghost_kaspa::wallet::WalletSecret =
        ghost_storage::open_json(password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    let mut manifest: GhostMediaManifest = required(request, "manifest")?;
    manifest.validate()?;
    let private_key =
        ghost_kaspa::wallet::private_key_for_address(&secret, &public, &manifest.creator)?;
    let signing = manifest.signing_bytes()?;
    manifest.creator_signature = ghost_kaspa::sign_domain_message(&private_key, DOMAIN, &signing)?;
    ghost_kaspa::verify_domain_message(
        &manifest.creator,
        &manifest.creator_signature,
        DOMAIN,
        &signing,
    )?;
    to_value(manifest)
}

fn import_local(args: &Value) -> Result<Value, String> {
    let content_type = normalized_content_type(required_str(args, "contentType")?)?;
    let bytes = STANDARD
        .decode(required_str(args, "dataBase64")?)
        .map_err(|error| format!("media base64 failed: {error}"))?;
    validate_local_size(&bytes)?;
    to_value(store_local_bytes(&content_type, &bytes)?)
}

pub(super) fn store_local_bytes(
    content_type: &str,
    bytes: &[u8],
) -> Result<MediaReference, String> {
    validate_local_size(bytes)?;
    let media_id = content_hash(bytes);
    MEDIA_CACHE.with(|cache| {
        cache.borrow_mut().insert(media_id.clone(), bytes.to_vec());
    });
    if bytes.len() <= MAX_PERSISTED_ASSET_BYTES {
        if let Ok(storage) = browser_storage() {
            let _ = storage.set_item(&media_key(&media_id), &STANDARD.encode(bytes));
        }
    }
    Ok(MediaReference {
        media_id: media_id.clone(),
        content_hash: media_id,
        content_type: content_type.to_owned(),
        size: bytes.len() as u64,
        location: MediaLocation::Local,
    })
}

async fn fetch_verified(args: &Value) -> Result<Value, String> {
    let reference: MediaReference = required(args, "reference")?;
    let bytes = match &reference.location {
        MediaLocation::Local => load_local(&reference)?,
        MediaLocation::Remote(url) => fetch_remote(url.as_str()).await?,
        MediaLocation::KaspaArchive(locator) => {
            super::runtime::archive::fetch(&reference, locator.as_str()).await?
        }
    };
    validate_bytes(&reference, &bytes)?;
    to_value(VerifiedMedia {
        content_type: reference.content_type,
        data_base64: STANDARD.encode(bytes),
    })
}

fn load_local(reference: &MediaReference) -> Result<Vec<u8>, String> {
    if let Some(bytes) = MEDIA_CACHE.with(|cache| cache.borrow().get(&reference.media_id).cloned())
    {
        return Ok(bytes);
    }
    let encoded = browser_storage()?
        .get_item(&media_key(&reference.media_id))
        .map_err(crate::native::invoke::js_error)?
        .ok_or_else(|| {
            "browser-local media is no longer available; re-import the downloaded recording"
                .to_string()
        })?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| format!("browser-local media cache is corrupt: {error}"))?;
    MEDIA_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(reference.media_id.clone(), bytes.clone());
    });
    Ok(bytes)
}

async fn fetch_remote(url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("remote browser media must use HTTPS".into());
    }
    let window = web_sys::window().ok_or("Browser window unavailable")?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(crate::native::invoke::js_error)?
        .dyn_into::<Response>()
        .map_err(|_| "media retrieval returned a non-HTTP response".to_string())?;
    if !response.ok() {
        return Err(format!(
            "media retrieval returned HTTP {}",
            response.status()
        ));
    }
    let buffer = JsFuture::from(
        response
            .array_buffer()
            .map_err(crate::native::invoke::js_error)?,
    )
    .await
    .map_err(crate::native::invoke::js_error)?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

fn validate_local_size(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_BROWSER_MEDIA_BYTES {
        Err("browser-local media is empty or exceeds the 512 MiB client limit".into())
    } else {
        Ok(())
    }
}

fn validate_bytes(reference: &MediaReference, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != reference.size {
        return Err("media size mismatch".into());
    }
    if !verify_content(bytes, &reference.content_hash) {
        return Err("media content hash mismatch".into());
    }
    Ok(())
}

fn normalized_content_type(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    let safe =
        value.starts_with("image/") || value.starts_with("audio/") || value.starts_with("video/");
    if safe && value.len() <= 96 && !value.contains(['\r', '\n']) {
        Ok(value)
    } else {
        Err("browser-local media content type is not supported".into())
    }
}

fn browser_storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or("Browser window unavailable")?
        .local_storage()
        .map_err(crate::native::invoke::js_error)?
        .ok_or_else(|| "Browser local storage is unavailable".to_string())
}

fn media_key(media_id: &str) -> String {
    format!("{MEDIA_KEY_PREFIX}{}", media_id.to_ascii_lowercase())
}
