use crate::profile_merge::merge_profile_patch;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};

const PROFILE_STATE_FILE: &str = "profiles.json";
const PROFILE_STATE_BACKUP_FILE: &str = "profiles.json.bak";
const PROFILE_STATE_TEMP_FILE: &str = "profiles.json.tmp";
const PROFILE_STATE_LOCK_FILE: &str = "profiles.json.lock";
const PROFILE_LOCK_RETRY_MS: u64 = 10;
const PROFILE_LOCK_ATTEMPTS: usize = 300;
const PROFILE_LOCK_STALE_SECS: u64 = 30;
const MAX_PROFILE_STATE_BYTES: usize = 8 * 1024 * 1024;

mod locking;
use locking::acquire_profile_state_lock;

fn profile_state_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::storage_root::data_root(app)?.join(PROFILE_STATE_FILE))
}

fn backup_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Ghost Talk profile state path has no parent".to_string())?;
    Ok(parent.join(PROFILE_STATE_BACKUP_FILE))
}

fn temp_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Ghost Talk profile state path has no parent".to_string())?;
    Ok(parent.join(PROFILE_STATE_TEMP_FILE))
}

fn read_profile_state(path: &Path) -> Result<Option<String>, String> {
    let Some(metadata) = optional_profile_metadata(path)? else {
        return Ok(None);
    };
    if metadata.len() > MAX_PROFILE_STATE_BYTES as u64 {
        return Err("Ghost Talk profile state exceeds the configured size limit".into());
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("read Ghost Talk profile state: {error}"))?;
    validate_profile_json(&raw)?;
    Ok(Some(raw))
}

fn optional_profile_metadata(path: &Path) -> Result<Option<fs::Metadata>, String> {
    fs::metadata(path).map(Some).or_else(|error| {
        (error.kind() == std::io::ErrorKind::NotFound)
            .then_some(None)
            .ok_or_else(|| format!("read Ghost Talk profile state metadata: {error}"))
    })
}

fn validate_profile_json(json: &str) -> Result<(), String> {
    if json.len() > MAX_PROFILE_STATE_BYTES {
        return Err("Ghost Talk profile state exceeds the configured size limit".into());
    }
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| format!("Ghost Talk profile state is invalid JSON: {error}"))?;
    let profiles = value
        .as_array()
        .ok_or_else(|| "Ghost Talk profile state must be a JSON array".to_string())?;
    let auto_login_count = profiles
        .iter()
        .filter(|profile| {
            profile
                .get("autoLogin")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        })
        .count();
    if auto_login_count > 1 {
        return Err(
            "Ghost Talk profile state is invalid: only one ID may enable automatic login".into(),
        );
    }
    Ok(())
}

fn recover_staged_profile_state(path: &Path) -> Result<Option<String>, String> {
    let temp = temp_path(path)?;
    let Some(raw) = read_profile_state(&temp).ok().flatten() else {
        return Ok(None);
    };

    // profile_state_save writes + fsyncs the complete temporary snapshot before
    // replacing the committed file. A valid leftover temp therefore represents
    // the newest visible state after a process interruption in that replacement
    // window. Prefer it over the older primary and best-effort promote it now.
    let backup = backup_path(path)?;
    remove_existing_best_effort(&backup);
    if !stage_existing_best_effort(path, &backup) {
        // The staged snapshot is still valid and readable. Return it even if this
        // filesystem will not let startup repair the filenames; a later save can.
        return Ok(Some(raw));
    }
    if !promote_staged_best_effort(&temp, path, &backup) {
        return Ok(Some(raw));
    }
    let _ = fs::remove_file(&backup);
    Ok(Some(raw))
}

fn remove_existing_best_effort(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn stage_existing_best_effort(path: &Path, backup: &Path) -> bool {
    !path.exists() || fs::rename(path, backup).is_ok()
}

fn promote_staged_best_effort(temp: &Path, path: &Path, backup: &Path) -> bool {
    let promoted = fs::rename(temp, path).is_ok();
    if !promoted && backup.exists() && !path.exists() {
        let _ = fs::rename(backup, path);
    }
    promoted
}

#[tauri::command]
pub fn profile_state_load(app: State<'_, crate::NativeAppState>) -> Result<Option<String>, String> {
    let app = app.handle();
    let path = profile_state_path(&app)?;
    let _lock = acquire_profile_state_lock(&path)?;
    if let Some(staged) = recover_staged_profile_state(&path)? {
        return Ok(Some(staged));
    }
    load_committed_or_backup(&path)
}

fn load_committed_or_backup(path: &Path) -> Result<Option<String>, String> {
    match read_profile_state(path) {
        Ok(Some(raw)) => Ok(Some(raw)),
        Ok(None) => read_profile_state(&backup_path(path)?),
        Err(primary_error) => recover_profile_backup(path, primary_error),
    }
}

fn recover_profile_backup(path: &Path, primary_error: String) -> Result<Option<String>, String> {
    match read_profile_state(&backup_path(path)?) {
        Ok(Some(raw)) => Ok(Some(raw)),
        Ok(None) => Err(primary_error),
        Err(backup_error) => Err(format!(
            "{primary_error}; Ghost Talk profile backup recovery also failed: {backup_error}"
        )),
    }
}

fn commit_profile_state(path: &Path, json: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Ghost Talk profile state path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create Ghost Talk data directory: {error}"))?;

    let temp = temp_path(path)?;
    let backup = backup_path(path)?;
    remove_existing(&temp, "remove stale Ghost Talk profile temp state")?;

    {
        let mut file = fs::File::create(&temp)
            .map_err(|error| format!("create Ghost Talk profile temp state: {error}"))?;
        file.write_all(json.as_bytes())
            .map_err(|error| format!("write Ghost Talk profile state: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync Ghost Talk profile state: {error}"))?;
    }

    remove_existing(&backup, "remove previous Ghost Talk profile backup")?;
    stage_existing(path, &backup)?;
    promote_temp(&temp, path, &backup)?;
    let _ = fs::remove_file(&backup);
    Ok(())
}

fn remove_existing(path: &Path, context: &str) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("{context}: {error}"))?;
    }
    Ok(())
}

fn stage_existing(path: &Path, backup: &Path) -> Result<(), String> {
    if path.exists() {
        fs::rename(path, backup)
            .map_err(|error| format!("stage previous Ghost Talk profile state: {error}"))?;
    }
    Ok(())
}

fn promote_temp(temp: &Path, path: &Path, backup: &Path) -> Result<(), String> {
    let Err(error) = fs::rename(temp, path) else {
        return Ok(());
    };
    if backup.exists() && !path.exists() {
        let _ = fs::rename(backup, path);
    }
    let _ = fs::remove_file(temp);
    Err(format!("commit Ghost Talk profile state: {error}"))
}

#[tauri::command]
pub fn profile_state_save(
    app: State<'_, crate::NativeAppState>,
    json: String,
    changed_profile_ids: Vec<String>,
) -> Result<(), String> {
    validate_profile_json(&json)?;
    if changed_profile_ids.is_empty() {
        return Ok(());
    }
    let app = app.handle();
    let path = profile_state_path(&app)?;
    let _lock = acquire_profile_state_lock(&path)?;
    let current = load_profile_for_merge(&path)?;
    let merged = merge_profile_patch(current.as_deref(), &json, &changed_profile_ids)?;
    commit_profile_state(&path, &merged)
}

fn load_profile_for_merge(path: &Path) -> Result<Option<String>, String> {
    if let Some(staged) = recover_staged_profile_state(path)? {
        return Ok(Some(staged));
    }
    match read_profile_state(path) {
        Ok(value) => Ok(value),
        Err(primary_error) => recover_profile_backup(path, primary_error),
    }
}

#[cfg(test)]
mod tests;
