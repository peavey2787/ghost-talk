pub(crate) use serde::{Deserialize, Serialize};

#[cfg(feature = "upstream")]
pub(crate) use hydra_crypto::{
    CryptoBackend, MlDsaKeyPair, MlDsaSigningKey, MlDsaVerificationKey, RustCryptoBackend,
};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) use std::{fs::OpenOptions, path::Path, time::UNIX_EPOCH};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) use sysinfo::{Pid, ProcessesToUpdate, System};
#[cfg(feature = "upstream")]
pub(crate) use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StegoProfile {
    Off,
    Deterministic,
    FastUnicode,
    FastHybrid,
    Arithmetic,
}

impl StegoProfile {
    pub fn parse_ui_label(value: &str) -> Result<Self, String> {
        [
            ("Off", Self::Off),
            ("Deterministic", Self::Deterministic),
            ("Fast Unicode", Self::FastUnicode),
            ("Fast Hybrid", Self::FastHybrid),
            ("Arithmetic", Self::Arithmetic),
        ]
        .into_iter()
        .find(|(name, _)| *name == value)
        .map(|(_, profile)| profile)
        .ok_or_else(|| "unknown HYDRA stego profile".into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContactProjection {
    pub handle: String,
    pub label: String,
    pub fingerprint: String,
    pub verified: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceivedProjection {
    pub from: String,
    pub plaintext: String,
    /// Optional authenticated application content type for structured payloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Exact authenticated KKTP session id when this plaintext came through the
    /// Ghost Talk KKTP wrapper. Direct HYDRA callers leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_sid: Option<String>,
}

impl ReceivedProjection {
    pub fn session_sid(&self) -> Option<&str> {
        self.session_sid.as_deref()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityProjection {
    pub id: String,
    pub label: String,
    pub unlocked: bool,
}

#[cfg(feature = "upstream")]
pub struct HydraFacade {
    pub(crate) inner: hydra_msg::Hydra,
    pub(crate) stego: hydra_stego::Stego,
    pub(crate) application_signing_key: Option<MlDsaSigningKey>,
    // Native builds own an OS advisory lease for the facade lifetime. Browser
    // builds instead persist the encrypted HYDRA snapshot in IndexedDB.
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    pub(crate) _profile_lease: NativeProfileLease,
    #[cfg(target_arch = "wasm32")]
    pub(crate) browser_persistence_name: String,
    #[cfg(target_arch = "wasm32")]
    pub(crate) browser_persistence_revision: u64,
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) struct NativeProfileLease {
    // Merely keeping this descriptor alive keeps the OS advisory lock alive.
    // The leading underscore is intentional: ownership, not reads/writes, is its job.
    pub(crate) _file: std::fs::File,
}

#[cfg(feature = "upstream")]
pub(crate) fn application_context_digest(context: &[u8]) -> Result<[u8; 64], String> {
    const DOMAIN: &[u8] = b"HYDRA-MSG/GhostTalk/KKTP/v2/context\0";
    let context_len =
        u64::try_from(context.len()).map_err(|_| "application context is too large".to_string())?;
    let mut input = Vec::with_capacity(DOMAIN.len() + 8 + context.len());
    input.extend_from_slice(DOMAIN);
    input.extend_from_slice(&context_len.to_be_bytes());
    input.extend_from_slice(context);
    Ok(RustCryptoBackend::sha3_512(&input))
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) const NATIVE_PROFILE_LOCK_FILE: &str = "state.hydra.lock";
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) const GHOST_TALK_PROFILE_LEASE_FILE: &str = "state.ghost-talk.profile.lease";

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn native_lock_pid(contents: &str) -> Option<u32> {
    let value = contents.trim().strip_prefix("pid=")?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// Own Ghost Talk's same-profile exclusion with a real OS advisory lock held
/// for the full lifetime of `HydraFacade`. The lease file may remain after a
/// crash, but the operating system releases the lock automatically when the
/// process exits, so file existence is never interpreted as ownership.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn acquire_native_profile_lease(
    profile_path: &Path,
) -> Result<NativeProfileLease, String> {
    std::fs::create_dir_all(profile_path)
        .map_err(|error| format!("could not create HYDRA profile directory: {error}"))?;
    let lease_path = profile_path.join(GHOST_TALK_PROFILE_LEASE_FILE);
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lease_path)
        .map_err(|error| format!("could not open Ghost Talk HYDRA profile lease: {error}"))?;
    fs2::FileExt::try_lock_exclusive(&lease).map_err(profile_lease_error)?;
    Ok(NativeProfileLease { _file: lease })
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn profile_lease_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        "HYDRA profile is already open in another Ghost Talk process".to_string()
    } else {
        format!("could not lock Ghost Talk HYDRA profile lease: {error}")
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn lock_modified_unix_seconds(lock_path: &Path) -> Result<u64, String> {
    let modified = std::fs::metadata(lock_path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("could not read HYDRA profile lock metadata: {error}"))?;
    modified
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "HYDRA profile lock has an invalid modification time".to_string())
}

/// Determine whether HYDRA's PID-only sentinel still names the *same process
/// instance* that created it. A bare PID is insufficient because operating
/// systems recycle process identifiers. `sysinfo` supplies the process start
/// time as Unix seconds on every platform Ghost Talk targets. If the current
/// process using that PID started after the sentinel was written, the PID was
/// recycled and the sentinel is stale even though that numeric PID is alive.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn process_instance_can_own_lock(process_started: u64, lock_written: u64) -> bool {
    // Both values are epoch seconds. A process that actually created the lock
    // cannot have started after that lock was written. Equality is treated as
    // owned because both clocks are second-granularity; that is the conservative
    // choice for the very small same-second PID-reuse ambiguity window.
    process_started <= lock_written
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn native_lock_owner_is_same_process(
    lock_path: &Path,
    pid: u32,
) -> Result<bool, String> {
    // `recover_stale_native_profile_lock` can only run while this process owns
    // the per-profile NativeProfileLease. Every Ghost Talk HYDRA open goes
    // through that lease. Therefore a sentinel carrying *our current numeric
    // PID* cannot belong to another live HydraFacade in this process: a second
    // facade would have failed lease acquisition first. It is a recycled PID
    // from an older process and is safe to reap. This also avoids the inherent
    // one-second ambiguity of timestamp-only PID-reuse detection for our PID.
    if pid == std::process::id() {
        return Ok(false);
    }
    if !sysinfo::IS_SUPPORTED_SYSTEM {
        return Err(
            "this platform cannot safely verify a upstream HYDRA profile lock owner".into(),
        );
    }

    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let Some(process) = system.process(pid) else {
        return Ok(false);
    };

    let lock_written = lock_modified_unix_seconds(lock_path)?;
    Ok(process_instance_can_own_lock(
        process.start_time(),
        lock_written,
    ))
}

/// Recover only HYDRA's upstream create-new PID sentinel, and only after Ghost
/// Talk has acquired its own long-lived profile lease. The upstream sentinel is
/// retained for compatibility with the pinned HYDRA crate, but its bare PID is
/// never trusted as ownership proof: PID reuse is checked against the current
/// process instance's start time before an apparently-live sentinel can block
/// the profile forever.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn recover_stale_native_profile_lock(
    profile_path: &Path,
    _lease: &NativeProfileLease,
) -> Result<(), String> {
    let lock_path = profile_path.join(NATIVE_PROFILE_LOCK_FILE);
    let Some(contents) = read_native_profile_lock(&lock_path)? else {
        return Ok(());
    };
    let pid = native_lock_pid(&contents).ok_or_else(|| {
        "HYDRA profile lock is malformed; refusing to remove it automatically".to_string()
    })?;
    if native_lock_owner_is_same_process(&lock_path, pid)? {
        return Err(format!(
            "HYDRA profile is already open by active process {pid}"
        ));
    }
    remove_native_profile_lock(&lock_path)
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn read_native_profile_lock(lock_path: &Path) -> Result<Option<String>, String> {
    std::fs::read_to_string(lock_path)
        .map(Some)
        .or_else(|error| optional_lock_file_error(error, "read"))
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn remove_native_profile_lock(lock_path: &Path) -> Result<(), String> {
    std::fs::remove_file(lock_path).or_else(|error| {
        (error.kind() == std::io::ErrorKind::NotFound)
            .then_some(())
            .ok_or_else(|| format!("could not remove stale HYDRA profile lock: {error}"))
    })
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn optional_lock_file_error(error: std::io::Error, action: &str) -> Result<Option<String>, String> {
    (error.kind() == std::io::ErrorKind::NotFound)
        .then_some(None)
        .ok_or_else(|| format!("could not {action} HYDRA profile lock: {error}"))
}

#[cfg(all(test, feature = "native", not(target_arch = "wasm32")))]
mod native_lock_tests {
    use super::{native_lock_pid, process_instance_can_own_lock};

    #[test]
    fn parses_upstream_pid_sentinel() {
        assert_eq!(native_lock_pid("pid=12345\n"), Some(12345));
        assert_eq!(native_lock_pid("pid=nope\n"), None);
        assert_eq!(native_lock_pid("12345\n"), None);
    }

    #[test]
    fn original_process_instance_can_own_lock() {
        assert!(process_instance_can_own_lock(1_000, 1_100));
        assert!(process_instance_can_own_lock(1_100, 1_100));
    }

    #[test]
    fn recycled_pid_cannot_own_older_lock() {
        assert!(!process_instance_can_own_lock(1_101, 1_100));
    }
}
