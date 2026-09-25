use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::io::show_backup;
use crate::model::{KasKoldBackupResult, Profile, WalletRecovery};

type BackupHandler = fn(
    &mut vault_runtime::VaultRuntime,
    &str,
    &[u8],
) -> Result<KasKoldBackupResult, String>;

const BACKUP_HANDLERS: [(&str, BackupHandler); 7] = [
    ("words", recovery_words),
    ("seedqr", seedqr),
    ("compact-seedqr", compact_seedqr),
    ("xprv", account_xprv),
    ("portable", portable),
    ("portable-xprv", portable_xprv),
    ("stego", stego),
];

pub(super) fn backup_callback(
    profile: Profile,
    password: String,
    kind: UseStateHandle<String>,
    secret: UseStateHandle<String>,
    carrier: UseStateHandle<Vec<u8>>,
    result: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Creating KasKold-compatible backup…".into());
        spawn_backup(BackupTask {
            profile: profile.clone(),
            password: password.clone(),
            kind: (*kind).clone(),
            secret: (*secret).clone(),
            carrier: (*carrier).clone(),
            result: result.clone(),
            status: status.clone(),
            busy: busy.clone(),
        });
    })
}

struct BackupTask {
    profile: Profile,
    password: String,
    kind: String,
    secret: String,
    carrier: Vec<u8>,
    result: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
}

fn spawn_backup(task: BackupTask) {
    spawn_local(async move {
        let response = match crate::controllers::account::reveal_recovery(&task.profile, &task.password).await {
            Ok(recovery) => create_backup(&recovery, &task.kind, &task.secret, &task.carrier),
            Err(error) => Err(error),
        };
        match response {
            Ok(value) => show_backup(value, &task.result, &task.status),
            Err(error) => task.status.set(error),
        }
        task.busy.set(false);
    });
}

fn create_backup(
    recovery: &WalletRecovery,
    kind: &str,
    credential: &str,
    carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    let mut runtime = vault_runtime::VaultRuntime::new();
    runtime
        .add_restored_wallet(&recovery.mnemonic, &recovery.passphrase)
        .map_err(runtime_error)?;
    let handler = BACKUP_HANDLERS
        .iter()
        .find_map(|(name, handler)| (*name == kind).then_some(*handler))
        .ok_or_else(|| "Unsupported KasKold backup kind".to_string())?;
    handler(&mut runtime, credential, carrier)
}

fn recovery_words(
    runtime: &mut vault_runtime::VaultRuntime,
    _credential: &str,
    _carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    text_backup(
        "kaskold-recovery-words.txt",
        runtime.backup_recovery_phrase().map_err(runtime_error)?.to_string(),
    )
}

fn seedqr(
    runtime: &mut vault_runtime::VaultRuntime,
    _credential: &str,
    _carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    byte_backup(
        "kaskold-seedqr.txt",
        "text/plain",
        runtime.backup_seedqr().map_err(runtime_error)?.to_vec(),
    )
}

fn compact_seedqr(
    runtime: &mut vault_runtime::VaultRuntime,
    _credential: &str,
    _carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    byte_backup(
        "kaskold-compact-seedqr.bin",
        "application/octet-stream",
        runtime.backup_compact_seedqr().map_err(runtime_error)?.to_vec(),
    )
}

fn account_xprv(
    runtime: &mut vault_runtime::VaultRuntime,
    _credential: &str,
    _carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    text_backup(
        "kaskold-account-xprv.txt",
        runtime.backup_account_xprv().map_err(runtime_error)?.to_string(),
    )
}

fn portable(
    runtime: &mut vault_runtime::VaultRuntime,
    credential: &str,
    _carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    require_backup_password(credential)?;
    byte_backup(
        "kaskold-vault-backup.kwp",
        "application/octet-stream",
        runtime.portable_backup(credential).map_err(runtime_error)?,
    )
}

fn portable_xprv(
    runtime: &mut vault_runtime::VaultRuntime,
    credential: &str,
    _carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    require_backup_password(credential)?;
    byte_backup(
        "kaskold-xprv-backup.kwp",
        "application/octet-stream",
        runtime.portable_xprv_backup(credential).map_err(runtime_error)?,
    )
}

fn stego(
    runtime: &mut vault_runtime::VaultRuntime,
    credential: &str,
    carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    require_backup_password(credential)?;
    if carrier.is_empty() {
        return Err("Choose a JPEG carrier before creating a steganographic backup.".into());
    }
    byte_backup(
        "kaskold-steganographic-backup.jpg",
        "image/jpeg",
        runtime.stego_backup(carrier, credential).map_err(runtime_error)?,
    )
}

fn require_backup_password(value: &str) -> Result<(), String> {
    if value.is_empty() {
        Err("Enter a backup password for encrypted KasKold backup formats.".into())
    } else {
        Ok(())
    }
}

fn runtime_error(error: vault_runtime::VaultRuntimeError) -> String {
    format!("KasKold backup: {error:?}")
}

fn text_backup(filename: &str, text: String) -> Result<KasKoldBackupResult, String> {
    Ok(KasKoldBackupResult {
        filename: filename.into(),
        media_type: "text/plain".into(),
        text: Some(text),
        bytes: vec![],
    })
}

fn byte_backup(
    filename: &str,
    media_type: &str,
    bytes: Vec<u8>,
) -> Result<KasKoldBackupResult, String> {
    Ok(KasKoldBackupResult {
        filename: filename.into(),
        media_type: media_type.into(),
        text: None,
        bytes,
    })
}
