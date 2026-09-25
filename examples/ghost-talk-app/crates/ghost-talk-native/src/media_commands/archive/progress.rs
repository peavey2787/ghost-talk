use ghost_kaspa::{ArchiveProgress, ARCHIVE_PROGRESS_VERSION};
use ghost_media::content_hash;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use tauri::AppHandle;

pub(super) fn load(
    app: &AppHandle,
    profile_id: &str,
    media_id: &str,
) -> Result<Option<ArchiveProgress>, String> {
    let path = progress_path(app, profile_id, media_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| format!("read archive progress: {error}"))?;
    let progress: ArchiveProgress = serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode archive progress: {error}"))?;
    if progress.version != ARCHIVE_PROGRESS_VERSION {
        return Err("archive progress version is unsupported".into());
    }
    Ok(Some(progress))
}

pub(super) fn save(app: &AppHandle, progress: &ArchiveProgress) -> Result<(), String> {
    let path = progress_path(app, &progress.profile_id, &progress.media_id)?;
    let bytes = serde_json::to_vec(progress)
        .map_err(|error| format!("encode archive progress: {error}"))?;
    atomic_write(&path, &bytes)
}

pub(super) fn remove(app: &AppHandle, profile_id: &str, media_id: &str) -> Result<(), String> {
    let path = progress_path(app, profile_id, media_id)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("remove archive progress: {error}"))?;
    }
    Ok(())
}

pub(super) fn validate(
    progress: &ArchiveProgress,
    profile_id: &str,
    content_type: &str,
    address: &str,
    bytes: &[u8],
    chunks: &[&[u8]],
) -> Result<(), String> {
    validate_identity(progress, profile_id, content_type, address, bytes, chunks)?;
    validate_completed(progress, chunks)?;
    validate_in_flight(progress, chunks)
}

fn validate_identity(
    progress: &ArchiveProgress,
    profile_id: &str,
    content_type: &str,
    address: &str,
    bytes: &[u8],
    chunks: &[&[u8]],
) -> Result<(), String> {
    let matches = progress.profile_id == profile_id
        && progress.content_type == content_type
        && progress.address == address
        && progress.total_size == bytes.len() as u64
        && progress.media_id == content_hash(bytes)
        && progress.chunk_count as usize == chunks.len();
    matches
        .then_some(())
        .ok_or_else(|| "saved archive progress does not match the requested media".into())
}

fn validate_completed(progress: &ArchiveProgress, chunks: &[&[u8]]) -> Result<(), String> {
    if progress.completed.len() != progress.transaction_ids.len() {
        return Err("saved archive progress transaction list is inconsistent".into());
    }
    for (position, completed) in progress.completed.iter().enumerate() {
        let body = chunks
            .get(position)
            .ok_or("saved archive progress exceeds media chunks")?;
        let matches = completed.index == position as u32
            && completed.transaction_id == progress.transaction_ids[position]
            && completed.size as usize == body.len()
            && completed.content_hash == content_hash(body);
        if !matches {
            return Err("saved archive progress chunk metadata is inconsistent".into());
        }
    }
    Ok(())
}

fn validate_in_flight(progress: &ArchiveProgress, chunks: &[&[u8]]) -> Result<(), String> {
    let Some(in_flight) = &progress.in_flight else {
        return Ok(());
    };
    let position = progress.completed.len();
    let body = chunks
        .get(position)
        .ok_or("saved archive in-flight chunk is out of range")?;
    let matches = in_flight.index == position as u32
        && in_flight.size as usize == body.len()
        && in_flight.content_hash == content_hash(body);
    matches
        .then_some(())
        .ok_or_else(|| "saved archive in-flight metadata is inconsistent".into())
}

fn progress_path(app: &AppHandle, profile_id: &str, media_id: &str) -> Result<PathBuf, String> {
    let root = crate::persistence::storage_root::data_root(app)?.join("media/archive-progress");
    fs::create_dir_all(&root)
        .map_err(|error| format!("create archive-progress directory: {error}"))?;
    let mut digest = Sha256::new();
    digest.update(profile_id.as_bytes());
    digest.update([0]);
    digest.update(media_id.as_bytes());
    Ok(root.join(format!("{}.json", hex::encode(digest.finalize()))))
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    fs::write(&temp, bytes).map_err(|error| format!("write archive progress: {error}"))?;
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|error| format!("backup archive progress: {error}"))?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(format!("commit archive progress: {error}"));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

#[cfg(test)]
mod tests;
