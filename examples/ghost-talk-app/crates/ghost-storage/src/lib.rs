#![forbid(unsafe_code)]

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use argon2::Argon2;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use zeroize::Zeroize;

const MAGIC: &[u8; 4] = b"GTVL";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SealedVault(pub Vec<u8>);

pub fn seal(password: &str, plaintext: &[u8]) -> Result<SealedVault, String> {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut key)
        .map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| "vault encryption failed".to_string())?;
    key.fill(0);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(SealedVault(out))
}

pub fn open(password: &str, vault: &SealedVault) -> Result<Vec<u8>, String> {
    let b = &vault.0;
    if b.len() < 32 || &b[..4] != MAGIC {
        return Err("invalid vault".into());
    }
    let salt = &b[4..20];
    let nonce = &b[20..32];
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let result = cipher
        .decrypt(Nonce::from_slice(nonce), &b[32..])
        .map_err(|_| "vault authentication failed".to_string());
    key.fill(0);
    result
}

/// Serialize and seal one typed secret while zeroizing its temporary JSON bytes.
pub fn seal_json<T: Serialize>(password: &str, value: &T, label: &str) -> Result<Vec<u8>, String> {
    let mut plaintext =
        serde_json::to_vec(value).map_err(|error| format!("{label} encode: {error}"))?;
    let result = seal(password, &plaintext).map(|sealed| sealed.0);
    plaintext.zeroize();
    result
}

/// Open and deserialize one typed secret while zeroizing its temporary JSON bytes.
pub fn open_json<T: DeserializeOwned>(
    password: &str,
    sealed: &[u8],
    label: &str,
) -> Result<T, String> {
    let mut plaintext = open(password, &SealedVault(sealed.to_vec()))?;
    let result =
        serde_json::from_slice(&plaintext).map_err(|error| format!("{label} decode: {error}"));
    plaintext.zeroize();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_authenticates() {
        let v = seal("pin", b"secret").unwrap();
        assert_eq!(open("pin", &v).unwrap(), b"secret");
        assert!(open("bad", &v).is_err())
    }
}
