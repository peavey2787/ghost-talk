use ghost_media::{verify_content, KaspaArchiveChunk};
use std::{fs, path::PathBuf};
use tauri::AppHandle;

pub(super) fn load_chunk(
    app: &AppHandle,
    chunk: &KaspaArchiveChunk,
) -> Result<Option<Vec<u8>>, String> {
    let path = chunk_path(app, &chunk.content_hash)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| format!("read media chunk cache: {error}"))?;
    if bytes.len() == chunk.size as usize && verify_content(&bytes, &chunk.content_hash) {
        return Ok(Some(bytes));
    }
    let _ = fs::remove_file(path);
    Ok(None)
}

pub(super) fn store_chunk(
    app: &AppHandle,
    chunk: &KaspaArchiveChunk,
    bytes: &[u8],
) -> Result<(), String> {
    if bytes.len() != chunk.size as usize || !verify_content(bytes, &chunk.content_hash) {
        return Err("refusing to cache an unverified archive chunk".into());
    }
    let path = chunk_path(app, &chunk.content_hash)?;
    if !path.exists() {
        fs::write(path, bytes).map_err(|error| format!("store media chunk cache: {error}"))?;
    }
    Ok(())
}

fn chunk_path(app: &AppHandle, hash: &str) -> Result<PathBuf, String> {
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("archive chunk hash is invalid".into());
    }
    let root = crate::persistence::storage_root::data_root(app)?.join("media/archive-chunks");
    fs::create_dir_all(&root).map_err(|error| format!("create media chunk cache: {error}"))?;
    Ok(root.join(format!("{}.bin", hash.to_ascii_lowercase())))
}
