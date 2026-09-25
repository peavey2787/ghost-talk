use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_media::{content_hash, MediaLocation, MediaReference};
use std::{fs, path::PathBuf};
use tauri::AppHandle;

const MAX_LOCAL_MEDIA_BYTES: usize = 512 * 1024 * 1024;

pub(crate) fn media_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = crate::persistence::storage_root::data_root(app)?.join("media");
    fs::create_dir_all(&path).map_err(|error| format!("create media directory: {error}"))?;
    Ok(path)
}

pub(crate) fn local_path(app: &AppHandle, media_id: &str) -> Result<PathBuf, String> {
    if media_id.len() != 64 || !media_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("local media id is invalid".into());
    }
    Ok(media_dir(app)?.join(format!("{}.bin", media_id.to_ascii_lowercase())))
}

pub(crate) fn store_bytes(
    app: &AppHandle,
    content_type: &str,
    bytes: &[u8],
) -> Result<MediaReference, String> {
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_MEDIA_BYTES {
        return Err("local media is empty or exceeds the 512 MiB limit".into());
    }
    let content_type = normalized_content_type(content_type)?;
    let media_id = content_hash(bytes);
    let path = local_path(app, &media_id)?;
    let rewrite = if path.exists() {
        fs::read(&path)
            .map(|current| current != bytes)
            .unwrap_or(true)
    } else {
        true
    };
    if rewrite {
        fs::write(&path, bytes).map_err(|error| format!("store local media: {error}"))?;
    }
    Ok(MediaReference {
        media_id: media_id.clone(),
        content_hash: media_id,
        content_type,
        size: bytes.len() as u64,
        location: MediaLocation::Local,
    })
}

pub(crate) fn load_bytes(app: &AppHandle, media_id: &str) -> Result<Vec<u8>, String> {
    let bytes = fs::read(local_path(app, media_id)?)
        .map_err(|error| format!("read local media: {error}"))?;
    if bytes.len() > MAX_LOCAL_MEDIA_BYTES {
        return Err("local media exceeds the 512 MiB limit".into());
    }
    Ok(bytes)
}

pub(crate) fn load_if_present(app: &AppHandle, media_id: &str) -> Result<Option<Vec<u8>>, String> {
    let path = local_path(app, media_id)?;
    if !path.exists() {
        return Ok(None);
    }
    load_bytes(app, media_id).map(Some)
}

pub(crate) fn remove_if_present(app: &AppHandle, media_id: &str) -> Result<(), String> {
    let path = local_path(app, media_id)?;
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("remove corrupt local media cache: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn media_import_local(
    app: AppHandle,
    content_type: String,
    data_base64: String,
) -> Result<MediaReference, String> {
    let bytes = STANDARD
        .decode(data_base64)
        .map_err(|error| format!("media base64 failed: {error}"))?;
    store_bytes(&app, &content_type, &bytes)
}

fn normalized_content_type(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    let safe =
        value.starts_with("image/") || value.starts_with("audio/") || value.starts_with("video/");
    if !safe || value.len() > 96 || value.contains(['\r', '\n']) {
        Err("media content type is not supported".into())
    } else {
        Ok(value)
    }
}
