#![forbid(unsafe_code)]

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use argon2::Argon2;
use serde::{Deserialize, Serialize};

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
