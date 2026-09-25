use aes_gcm::{aead::{Aead, Payload}, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;
use zeroize::Zeroize;

use super::util::{required, required_str};

const DEVICE_KEY: &str = "ghost-talk.remembered-unlock.device-key.v1";
const CREDENTIAL_PREFIX: &str = "ghost-talk.remembered-unlock.credential.v1.";
const AAD_PREFIX: &str = "GhostTalk/RememberedUnlock/Web/v1/";
const MAX_PASSWORD: usize = 4096;

pub(in crate::native::browser_host) fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "remembered_unlock_set" => set(args),
        "remembered_unlock_load" => load(args),
        _ => Err(format!("unknown browser credential command: {command}")),
    }
}

fn set(args: &Value) -> Result<Value, String> {
    let profile_id = profile_id(args)?;
    let enabled = args.get("enabled").and_then(Value::as_bool)
        .ok_or("browser command argument enabled is missing or not a boolean")?;
    if !enabled {
        super::storage::local_storage()?.remove_item(&credential_key(profile_id)).map_err(crate::native::invoke::js_error)?;
        return Ok(Value::Null);
    }
    let password = required_str(args, "password")?;
    if password.len() < 8 || password.len() > MAX_PASSWORD {
        return Err("remembered unlock password length is invalid".into());
    }
    let sealed: Vec<u8> = required(args, "sealed")?;
    let projection: crate::model::WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let secret: WalletSecret = ghost_storage::open_json(password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    drop(secret);
    remove_other_credentials(profile_id)?;
    store_password(profile_id, password)?;
    Ok(Value::Null)
}

fn load(args: &Value) -> Result<Value, String> {
    let profile_id = profile_id(args)?;
    let Some(encoded) = super::storage::local_storage()?.get_item(&credential_key(profile_id)).map_err(crate::native::invoke::js_error)? else {
        return Ok(Value::Null);
    };
    let blob = STANDARD.decode(encoded).map_err(|_| "remembered-unlock credential is invalid")?;
    if blob.len() < 12 + 16 || blob.len() > 12 + MAX_PASSWORD + 16 {
        return Err("remembered-unlock credential is invalid".into());
    }
    let mut key = device_key(false)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "remembered-unlock key initialization failed")?;
    let aad = format!("{AAD_PREFIX}{profile_id}");
    let plaintext = cipher.decrypt(Nonce::from_slice(&blob[..12]), Payload { msg: &blob[12..], aad: aad.as_bytes() })
        .map_err(|_| "remembered-unlock credential authentication failed".to_string());
    key.zeroize();
    let mut plaintext = plaintext?;
    let password = String::from_utf8(plaintext.clone()).map_err(|_| "remembered-unlock credential is not UTF-8".to_string())?;
    plaintext.zeroize();
    Ok(Value::String(password))
}

fn store_password(profile_id: &str, password: &str) -> Result<(), String> {
    let mut key = device_key(true)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "remembered-unlock key initialization failed")?;
    let mut nonce = [0u8; 12]; OsRng.fill_bytes(&mut nonce);
    let aad = format!("{AAD_PREFIX}{profile_id}");
    let encrypted = cipher.encrypt(Nonce::from_slice(&nonce), Payload { msg: password.as_bytes(), aad: aad.as_bytes() })
        .map_err(|_| "remembered-unlock encryption failed".to_string());
    key.zeroize();
    let mut blob = nonce.to_vec(); blob.extend_from_slice(&encrypted?);
    super::storage::local_storage()?.set_item(&credential_key(profile_id), &STANDARD.encode(blob)).map_err(crate::native::invoke::js_error)
}

fn device_key(create: bool) -> Result<[u8; 32], String> {
    let storage = super::storage::local_storage()?;
    if let Some(encoded) = storage.get_item(DEVICE_KEY).map_err(crate::native::invoke::js_error)? {
        let bytes = STANDARD.decode(encoded).map_err(|_| "remembered-unlock device key is invalid")?;
        return bytes.try_into().map_err(|_| "remembered-unlock device key has invalid length".into());
    }
    if !create { return Err("remembered-unlock device key is unavailable".into()); }
    let mut key = [0u8; 32]; OsRng.fill_bytes(&mut key);
    storage.set_item(DEVICE_KEY, &STANDARD.encode(key)).map_err(crate::native::invoke::js_error)?;
    Ok(key)
}

fn remove_other_credentials(keep: &str) -> Result<(), String> {
    let storage = super::storage::local_storage()?;
    let keep_key = credential_key(keep);
    let mut remove = Vec::new();
    for index in 0..storage.length().map_err(crate::native::invoke::js_error)? {
        if let Some(key) = storage.key(index).map_err(crate::native::invoke::js_error)? {
            if key.starts_with(CREDENTIAL_PREFIX) && key != keep_key { remove.push(key); }
        }
    }
    for key in remove { storage.remove_item(&key).map_err(crate::native::invoke::js_error)?; }
    Ok(())
}

fn credential_key(profile_id: &str) -> String {
    format!("{CREDENTIAL_PREFIX}{profile_id}")
}

fn profile_id(args: &Value) -> Result<&str, String> {
    let value = required_str(args, "profileId")?;
    if value.is_empty() || value.len() > 128 || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
        return Err("remembered-unlock profile id is invalid".into());
    }
    Ok(value)
}
