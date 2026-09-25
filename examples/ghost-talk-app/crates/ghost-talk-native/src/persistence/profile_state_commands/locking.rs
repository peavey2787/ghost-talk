use super::{
    PROFILE_LOCK_ATTEMPTS, PROFILE_LOCK_RETRY_MS, PROFILE_LOCK_STALE_SECS, PROFILE_STATE_LOCK_FILE,
};
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

struct ProfileStateLock {
    path: PathBuf,
}

impl Drop for ProfileStateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn acquire_profile_state_lock(path: &Path) -> Result<impl Sized, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Ghost Talk profile state path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create Ghost Talk data directory: {error}"))?;
    let lock = parent.join(PROFILE_STATE_LOCK_FILE);
    for _ in 0..PROFILE_LOCK_ATTEMPTS {
        if let Some(acquired) = try_acquire_profile_state_lock(&lock)? {
            return Ok(acquired);
        }
    }
    Err("Ghost Talk profile state is busy in another app instance; retry the action".into())
}

fn try_acquire_profile_state_lock(lock: &Path) -> Result<Option<ProfileStateLock>, String> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(lock) {
        Ok(file) => file,
        Err(error) => return handle_profile_lock_contention(lock, error),
    };
    let _ = writeln!(file, "{}", std::process::id());
    let _ = file.sync_all();
    Ok(Some(ProfileStateLock {
        path: lock.to_owned(),
    }))
}

fn handle_profile_lock_contention(
    lock: &Path,
    error: std::io::Error,
) -> Result<Option<ProfileStateLock>, String> {
    if error.kind() != std::io::ErrorKind::AlreadyExists {
        return Err(format!("lock Ghost Talk profile state: {error}"));
    }
    let stale = fs::metadata(lock)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= Duration::from_secs(PROFILE_LOCK_STALE_SECS));
    if stale {
        let _ = fs::remove_file(lock);
    } else {
        thread::sleep(Duration::from_millis(PROFILE_LOCK_RETRY_MS));
    }
    Ok(None)
}
