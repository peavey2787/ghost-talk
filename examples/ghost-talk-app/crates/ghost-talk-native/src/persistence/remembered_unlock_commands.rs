use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use ghost_kaspa::wallet::WalletPublic;
use rand::{rngs::OsRng, RngCore};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};
use zeroize::Zeroize;

const UNLOCK_DIR: &str = "remembered-unlock";
const DEVICE_KEY_FILE: &str = "device.key";
const MAGIC: &[u8; 5] = b"GTAU1";
const NONCE_BYTES: usize = 12;
const MAX_PASSWORD_BYTES: usize = 4096;

#[tauri::command]
pub async fn remembered_unlock_set(
    app: State<'_, crate::NativeAppState>,
    profile_id: String,
    enabled: bool,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<(), String> {
    let app = app.handle();
    crate::run_blocking("remembered unlock setting", move || {
        validate_profile_id(&profile_id)?;
        if !enabled {
            // Disabling automatic/passwordless login is a destructive local
            // credential cleanup operation. It must remain possible from the ID
            // chooser even when the user cannot (or does not want to) unlock.
            return remove_password(&app, &profile_id);
        }

        let secret = crate::wallet_commands::open_secret(&password, &sealed)?;
        crate::wallet_commands::validate_public_projection(&secret, &public)?;
        drop(secret);
        remove_other_passwords(&app, &profile_id)?;
        store_password(&app, &profile_id, &password)
    })
    .await
}

#[tauri::command]
pub async fn remembered_unlock_load(
    app: State<'_, crate::NativeAppState>,
    profile_id: String,
) -> Result<Option<String>, String> {
    let app = app.handle();
    crate::run_blocking("remembered unlock load", move || {
        validate_profile_id(&profile_id)?;
        load_password(&app, &profile_id)
    })
    .await
}

fn store_password(app: &AppHandle, profile_id: &str, password: &str) -> Result<(), String> {
    if password.len() < 8 || password.len() > MAX_PASSWORD_BYTES {
        return Err("remembered unlock password length is invalid".into());
    }
    let root = unlock_root(app)?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("create remembered-unlock directory: {error}"))?;
    let mut key = device_key(&root)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "remembered-unlock key initialization failed".to_string())?;
    let mut nonce = [0u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    let aad = aad(profile_id);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: password.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "remembered-unlock encryption failed".to_string());
    key.zeroize();
    let encrypted = encrypted?;
    let mut blob = Vec::with_capacity(MAGIC.len() + NONCE_BYTES + encrypted.len());
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&encrypted);
    write_private_atomic(&credential_path(&root, profile_id), &blob)
}

fn load_password(app: &AppHandle, profile_id: &str) -> Result<Option<String>, String> {
    let root = unlock_root(app)?;
    let Some(blob) = read_credential(&root, profile_id)? else {
        return Ok(None);
    };
    validate_credential_blob(&blob)?;
    let plaintext = decrypt_credential(&root, profile_id, &blob)?;
    credential_password(plaintext).map(Some)
}

fn read_credential(root: &Path, profile_id: &str) -> Result<Option<Vec<u8>>, String> {
    optional_fs_result(
        fs::read(credential_path(root, profile_id)),
        "read remembered-unlock credential",
    )
}

fn validate_credential_blob(blob: &[u8]) -> Result<(), String> {
    let min = MAGIC.len() + NONCE_BYTES + 16;
    let max = MAGIC.len() + NONCE_BYTES + MAX_PASSWORD_BYTES + 16;
    if blob.len() < min || blob.len() > max || &blob[..MAGIC.len()] != MAGIC {
        return Err("remembered-unlock credential is invalid".into());
    }
    Ok(())
}

fn decrypt_credential(root: &Path, profile_id: &str, blob: &[u8]) -> Result<Vec<u8>, String> {
    let mut key = read_device_key(root)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "remembered-unlock key initialization failed".to_string())?;
    let ciphertext_start = MAGIC.len() + NONCE_BYTES;
    let aad = aad(profile_id);
    let result = cipher
        .decrypt(
            Nonce::from_slice(&blob[MAGIC.len()..ciphertext_start]),
            Payload {
                msg: &blob[ciphertext_start..],
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "remembered-unlock credential authentication failed".to_string());
    key.zeroize();
    result
}

fn credential_password(plaintext: Vec<u8>) -> Result<String, String> {
    match String::from_utf8(plaintext) {
        Ok(password) => Ok(password),
        Err(error) => {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            Err("remembered-unlock credential is not UTF-8".into())
        }
    }
}

fn remove_password(app: &AppHandle, profile_id: &str) -> Result<(), String> {
    let root = unlock_root(app)?;
    let path = credential_path(&root, profile_id);
    optional_fs_result(fs::remove_file(path), "remove remembered-unlock credential").map(|_| ())
}

fn remove_other_passwords(app: &AppHandle, keep_profile_id: &str) -> Result<(), String> {
    let root = unlock_root(app)?;
    let Some(entries) = credential_entries(&root)? else {
        return Ok(());
    };
    let keep = format!("{keep_profile_id}.cred");
    for entry in entries {
        remove_other_credential(entry.map_err(|error| error.to_string())?, &keep)?;
    }
    Ok(())
}

fn credential_entries(root: &Path) -> Result<Option<fs::ReadDir>, String> {
    optional_fs_result(fs::read_dir(root), "read remembered-unlock directory")
}

fn optional_fs_result<T>(result: std::io::Result<T>, context: &str) -> Result<Option<T>, String> {
    result.map(Some).or_else(|error| {
        (error.kind() == std::io::ErrorKind::NotFound)
            .then_some(None)
            .ok_or_else(|| format!("{context}: {error}"))
    })
}

fn remove_other_credential(entry: fs::DirEntry, keep: &str) -> Result<(), String> {
    let file_type = entry
        .file_type()
        .map_err(|error| format!("read remembered-unlock entry type: {error}"))?;
    if !file_type.is_file() {
        return Ok(());
    }
    let name = entry.file_name();
    let name = name.to_string_lossy();
    if name.ends_with(".cred") && name != keep {
        fs::remove_file(entry.path())
            .map_err(|error| format!("remove previous remembered-unlock credential: {error}"))?;
    }
    Ok(())
}

fn unlock_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::storage_root::data_root(app)?.join(UNLOCK_DIR))
}

fn credential_path(root: &Path, profile_id: &str) -> PathBuf {
    root.join(format!("{profile_id}.cred"))
}

fn aad(profile_id: &str) -> String {
    format!("GhostTalk/remembered-unlock/v1:{profile_id}")
}

fn validate_profile_id(profile_id: &str) -> Result<(), String> {
    if profile_id.len() != 32 || !profile_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk profile id is invalid".into());
    }
    Ok(())
}

fn device_key(root: &Path) -> Result<[u8; 32], String> {
    match read_device_key(root) {
        Ok(key) => Ok(key),
        Err(error) if error == "remembered-unlock device key is missing" => {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            write_private_atomic(&root.join(DEVICE_KEY_FILE), &key)?;
            Ok(key)
        }
        Err(error) => Err(error),
    }
}

fn read_device_key(root: &Path) -> Result<[u8; 32], String> {
    let path = root.join(DEVICE_KEY_FILE);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err("remembered-unlock device key is missing".into())
        }
        Err(error) => return Err(format!("read remembered-unlock device key: {error}")),
    };
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "remembered-unlock device key has invalid length".to_string())?;
    Ok(key)
}

fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "remembered-unlock path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create remembered-unlock directory: {error}"))?;
    let temp = path.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("create remembered-unlock temp file: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("write remembered-unlock data: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync remembered-unlock data: {error}"))?;
    drop(file);
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("replace remembered-unlock data: {error}"))?;
    }
    fs::rename(&temp, path).map_err(|error| format!("commit remembered-unlock data: {error}"))?;
    Ok(())
}
