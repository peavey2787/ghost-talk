use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use rand::{rngs::OsRng, RngCore};
use ghost_kaspa::wallet::WalletPublic;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use zeroize::Zeroize;

const UNLOCK_DIR: &str = "remembered-unlock-v1";
const DEVICE_KEY_FILE: &str = "device.key";
const MAGIC: &[u8; 5] = b"GTAU1";
const NONCE_BYTES: usize = 12;
const MAX_PASSWORD_BYTES: usize = 4096;

#[tauri::command]
pub async fn remembered_unlock_set(
    app: AppHandle,
    profile_id: String,
    enabled: bool,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<(), String> {
    super::run_blocking("remembered unlock setting", move || {
        validate_profile_id(&profile_id)?;
        let secret = super::wallet_commands::open_secret(&password, &sealed)?;
        super::wallet_commands::validate_public_projection(&secret, &public)?;
        drop(secret);
        if enabled {
            store_password(&app, &profile_id, &password)
        } else {
            remove_password(&app, &profile_id)
        }
    })
    .await
}

#[tauri::command]
pub async fn remembered_unlock_load(
    app: AppHandle,
    profile_id: String,
) -> Result<Option<String>, String> {
    super::run_blocking("remembered unlock load", move || {
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
    let path = credential_path(&root, profile_id);
    let blob = match fs::read(&path) {
        Ok(blob) => blob,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read remembered-unlock credential: {error}")),
    };
    if blob.len() < MAGIC.len() + NONCE_BYTES + 16
        || blob.len() > MAGIC.len() + NONCE_BYTES + MAX_PASSWORD_BYTES + 16
        || &blob[..MAGIC.len()] != MAGIC
    {
        return Err("remembered-unlock credential is invalid".into());
    }
    let mut key = read_device_key(&root)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "remembered-unlock key initialization failed".to_string())?;
    let nonce_start = MAGIC.len();
    let ciphertext_start = nonce_start + NONCE_BYTES;
    let aad = aad(profile_id);
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&blob[nonce_start..ciphertext_start]),
            Payload {
                msg: &blob[ciphertext_start..],
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "remembered-unlock credential authentication failed".to_string())?;
    key.zeroize();
    match String::from_utf8(plaintext) {
        Ok(password) => Ok(Some(password)),
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
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove remembered-unlock credential: {error}")),
    }
}

fn unlock_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Ghost Talk app-data directory: {error}"))?;
    Ok(root.join(UNLOCK_DIR))
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
    fs::rename(&temp, path)
        .map_err(|error| format!("commit remembered-unlock data: {error}"))?;
    Ok(())
}
