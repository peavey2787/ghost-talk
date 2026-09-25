use ghost_api::KasKoldBackupResult;
use vault_runtime::VaultRuntime;

use crate::runtime_error;

pub fn import_text(
    runtime: &mut VaultRuntime,
    kind: &str,
    value: &str,
    passphrase: &str,
) -> Result<(), String> {
    if import_recovery_text(runtime, kind, value, passphrase)? {
        return Ok(());
    }
    import_key_text(runtime, kind, value)
}

fn import_recovery_text(
    runtime: &mut VaultRuntime,
    kind: &str,
    value: &str,
    passphrase: &str,
) -> Result<bool, String> {
    match kind {
        "mnemonic" => runtime
            .add_restored_wallet(value, passphrase)
            .map(|_| true)
            .map_err(runtime_error),
        "seedqr" => runtime
            .add_recovery_material(value.as_bytes(), passphrase)
            .map(|_| true)
            .map_err(runtime_error),
        _ => Ok(false),
    }
}

fn import_key_text(runtime: &mut VaultRuntime, kind: &str, value: &str) -> Result<(), String> {
    match kind {
        "xprv" => runtime
            .add_account_xprv(value)
            .map(|_| ())
            .map_err(runtime_error),
        "raw-key" => runtime
            .add_raw_private_key(value)
            .map(|_| ())
            .map_err(runtime_error),
        _ => Err("Unsupported KasKold text import kind".into()),
    }
}

pub fn import_bytes(
    runtime: &mut VaultRuntime,
    kind: &str,
    data: &[u8],
    credential: &str,
) -> Result<(), String> {
    if import_encrypted_bytes(runtime, kind, data, credential)? {
        return Ok(());
    }
    import_recovery_bytes(runtime, kind, data, credential)
}

fn import_encrypted_bytes(
    runtime: &mut VaultRuntime,
    kind: &str,
    data: &[u8],
    credential: &str,
) -> Result<bool, String> {
    match kind {
        "portable" => runtime
            .add_portable_backup(data, credential)
            .map(|_| true)
            .map_err(runtime_error),
        "stego" => runtime
            .add_stego_backup(data, credential)
            .map(|_| true)
            .map_err(runtime_error),
        _ => Ok(false),
    }
}

fn import_recovery_bytes(
    runtime: &mut VaultRuntime,
    kind: &str,
    data: &[u8],
    credential: &str,
) -> Result<(), String> {
    if kind != "recovery" {
        return Err("Unsupported KasKold file import kind".into());
    }
    runtime
        .add_recovery_material(data, credential)
        .map(|_| ())
        .map_err(runtime_error)
}

pub fn backup(
    runtime: &VaultRuntime,
    kind: &str,
    credential: &str,
    carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    if let Some(result) = recovery_backup(runtime, kind)? {
        return Ok(result);
    }
    if let Some(result) = key_backup(runtime, kind)? {
        return Ok(result);
    }
    protected_backup(runtime, kind, credential, carrier)
}

fn recovery_backup(
    runtime: &VaultRuntime,
    kind: &str,
) -> Result<Option<KasKoldBackupResult>, String> {
    match kind {
        "words" => Ok(Some(text_backup(
            "kaskold-recovery-words.txt",
            runtime
                .backup_recovery_phrase()
                .map_err(runtime_error)?
                .to_string(),
        ))),
        "seedqr" => Ok(Some(byte_backup(
            "kaskold-seedqr.txt",
            "text/plain",
            runtime.backup_seedqr().map_err(runtime_error)?.to_vec(),
        ))),
        _ => Ok(None),
    }
}

fn key_backup(runtime: &VaultRuntime, kind: &str) -> Result<Option<KasKoldBackupResult>, String> {
    match kind {
        "compact-seedqr" => Ok(Some(byte_backup(
            "kaskold-compact-seedqr.bin",
            "application/octet-stream",
            runtime
                .backup_compact_seedqr()
                .map_err(runtime_error)?
                .to_vec(),
        ))),
        "xprv" => Ok(Some(text_backup(
            "kaskold-account-xprv.txt",
            runtime
                .backup_account_xprv()
                .map_err(runtime_error)?
                .to_string(),
        ))),
        _ => Ok(None),
    }
}

fn protected_backup(
    runtime: &VaultRuntime,
    kind: &str,
    credential: &str,
    carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    if let Some(result) = portable_backup(runtime, kind, credential)? {
        return Ok(result);
    }
    stego_backup(runtime, kind, credential, carrier)
}

fn portable_backup(
    runtime: &VaultRuntime,
    kind: &str,
    credential: &str,
) -> Result<Option<KasKoldBackupResult>, String> {
    match kind {
        "portable" => Ok(Some(byte_backup(
            "kaskold-vault-backup.kwp",
            "application/octet-stream",
            runtime.portable_backup(credential).map_err(runtime_error)?,
        ))),
        "portable-xprv" => Ok(Some(byte_backup(
            "kaskold-xprv-backup.kwp",
            "application/octet-stream",
            runtime
                .portable_xprv_backup(credential)
                .map_err(runtime_error)?,
        ))),
        _ => Ok(None),
    }
}

fn stego_backup(
    runtime: &VaultRuntime,
    kind: &str,
    credential: &str,
    carrier: &[u8],
) -> Result<KasKoldBackupResult, String> {
    if kind != "stego" {
        return Err("Unsupported KasKold backup kind".into());
    }
    Ok(byte_backup(
        "kaskold-steganographic-backup.jpg",
        "image/jpeg",
        runtime
            .stego_backup(carrier, credential)
            .map_err(runtime_error)?,
    ))
}

fn text_backup(filename: &str, text: String) -> KasKoldBackupResult {
    KasKoldBackupResult {
        filename: filename.into(),
        media_type: "text/plain".into(),
        text: Some(text),
        bytes: vec![],
    }
}

fn byte_backup(filename: &str, media_type: &str, bytes: Vec<u8>) -> KasKoldBackupResult {
    KasKoldBackupResult {
        filename: filename.into(),
        media_type: media_type.into(),
        text: None,
        bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn mnemonic_runtime() -> VaultRuntime {
        let mut runtime = VaultRuntime::new();
        import_text(&mut runtime, "mnemonic", MNEMONIC, "").expect("mnemonic imports");
        runtime
    }

    #[test]
    fn text_import_dispatch_covers_recovery_keys_and_rejection() {
        let mut runtime = VaultRuntime::new();
        import_text(&mut runtime, "mnemonic", MNEMONIC, "").expect("mnemonic imports");
        import_text(&mut runtime, "seedqr", MNEMONIC, "").expect("SeedQR text imports");
        let xprv = runtime.backup_account_xprv().expect("xprv export");
        let mut key_runtime = VaultRuntime::new();
        import_text(&mut key_runtime, "xprv", &xprv, "").expect("xprv imports");
        import_text(&mut key_runtime, "raw-key", &format!("{:064x}", 1u8), "")
            .expect("raw key imports");
        assert!(import_text(&mut key_runtime, "unknown", "", "").is_err());
    }

    #[test]
    fn byte_import_dispatch_covers_portable_recovery_and_rejection() {
        let source = mnemonic_runtime();
        let portable = source.portable_backup("pw").expect("portable backup");
        let mut runtime = VaultRuntime::new();
        import_bytes(&mut runtime, "portable", &portable, "pw").expect("portable imports");
        import_bytes(&mut runtime, "recovery", MNEMONIC.as_bytes(), "")
            .expect("recovery material imports");
        assert!(import_bytes(&mut runtime, "unknown", b"", "").is_err());
    }

    #[test]
    fn backup_dispatch_covers_plain_and_portable_formats() {
        let runtime = mnemonic_runtime();
        for kind in ["words", "seedqr", "compact-seedqr", "xprv"] {
            assert!(!backup(&runtime, kind, "pw", b"")
                .expect("plain backup")
                .filename
                .is_empty());
        }
        for kind in ["portable", "portable-xprv"] {
            assert!(!backup(&runtime, kind, "pw", b"")
                .expect("portable backup")
                .bytes
                .is_empty());
        }
        assert!(backup(&runtime, "unknown", "pw", b"").is_err());
    }
}
