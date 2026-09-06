use std::{fs, io::Write, path::{Path, PathBuf}};
use tauri::{AppHandle, Manager};

const PROFILE_STATE_FILE: &str = "profiles-v2.json";
const PROFILE_STATE_BACKUP_FILE: &str = "profiles-v2.json.bak";
const PROFILE_STATE_TEMP_FILE: &str = "profiles-v2.json.tmp";
const MAX_PROFILE_STATE_BYTES: usize = 8 * 1024 * 1024;

fn profile_state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Ghost Talk app-data directory: {error}"))?;
    Ok(root.join(PROFILE_STATE_FILE))
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
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read Ghost Talk profile state metadata: {error}")),
    };
    if metadata.len() > MAX_PROFILE_STATE_BYTES as u64 {
        return Err("Ghost Talk profile state exceeds the configured size limit".into());
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("read Ghost Talk profile state: {error}"))?;
    validate_profile_json(&raw)?;
    Ok(Some(raw))
}

fn validate_profile_json(json: &str) -> Result<(), String> {
    if json.len() > MAX_PROFILE_STATE_BYTES {
        return Err("Ghost Talk profile state exceeds the configured size limit".into());
    }
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| format!("Ghost Talk profile state is invalid JSON: {error}"))?;
    if !value.is_array() {
        return Err("Ghost Talk profile state must be a JSON array".into());
    }
    Ok(())
}

fn recover_staged_profile_state(path: &Path) -> Result<Option<String>, String> {
    let temp = temp_path(path)?;
    let raw = match read_profile_state(&temp) {
        Ok(Some(raw)) => raw,
        Ok(None) | Err(_) => return Ok(None),
    };

    // profile_state_save writes + fsyncs the complete temporary snapshot before
    // replacing the committed file. A valid leftover temp therefore represents
    // the newest visible state after a process interruption in that replacement
    // window. Prefer it over the older primary and best-effort promote it now.
    let backup = backup_path(path)?;
    if backup.exists() {
        let _ = fs::remove_file(&backup);
    }
    if path.exists() && fs::rename(path, &backup).is_err() {
        // The staged snapshot is still valid and readable. Return it even if this
        // filesystem will not let startup repair the filenames; a later save can.
        return Ok(Some(raw));
    }
    if fs::rename(&temp, path).is_err() {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Ok(Some(raw));
    }
    let _ = fs::remove_file(&backup);
    Ok(Some(raw))
}

#[tauri::command]
pub fn profile_state_load(app: AppHandle) -> Result<Option<String>, String> {
    let path = profile_state_path(&app)?;
    if let Some(staged) = recover_staged_profile_state(&path)? {
        return Ok(Some(staged));
    }
    match read_profile_state(&path) {
        Ok(Some(raw)) => Ok(Some(raw)),
        Ok(None) => {
            // A Windows replacement cannot atomically overwrite an existing file
            // with std::fs::rename. Keep the prior committed snapshot as a backup
            // and recover from it if the process stopped between replacement steps.
            read_profile_state(&backup_path(&path)?)
        }
        Err(primary_error) => {
            // Corruption after an interrupted write may still be recoverable from
            // the previous committed snapshot. Do not hide unrelated I/O failures
            // when there is no valid backup to recover.
            match read_profile_state(&backup_path(&path)?) {
                Ok(Some(raw)) => Ok(Some(raw)),
                Ok(None) => Err(primary_error),
                Err(backup_error) => Err(format!(
                    "{primary_error}; Ghost Talk profile backup recovery also failed: {backup_error}"
                )),
            }
        }
    }
}

#[tauri::command]
pub fn profile_state_save(app: AppHandle, json: String) -> Result<(), String> {
    validate_profile_json(&json)?;
    let path = profile_state_path(&app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Ghost Talk profile state path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create Ghost Talk app-data directory: {error}"))?;

    let temp = temp_path(&path)?;
    let backup = backup_path(&path)?;
    if temp.exists() {
        fs::remove_file(&temp)
            .map_err(|error| format!("remove stale Ghost Talk profile temp state: {error}"))?;
    }

    {
        let mut file = fs::File::create(&temp)
            .map_err(|error| format!("create Ghost Talk profile temp state: {error}"))?;
        file.write_all(json.as_bytes())
            .map_err(|error| format!("write Ghost Talk profile state: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync Ghost Talk profile state: {error}"))?;
    }

    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("remove previous Ghost Talk profile backup: {error}"))?;
    }
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| format!("stage previous Ghost Talk profile state: {error}"))?;
    }
    if let Err(error) = fs::rename(&temp, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        // A handled replacement failure is not an interrupted commit. Remove the
        // staged file so startup recovery only treats crash-leftover snapshots as
        // authoritative on r48 and later.
        let _ = fs::remove_file(&temp);
        return Err(format!("commit Ghost Talk profile state: {error}"));
    }

    // The primary file is now committed. Failure to remove the backup is not a
    // data-loss condition; leave it available for the next successful save.
    let _ = fs::remove_file(&backup);
    Ok(())
}
