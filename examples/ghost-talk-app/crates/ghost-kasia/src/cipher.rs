use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use secp256k1::{ecdh::shared_secret_point, PublicKey, Secp256k1, SecretKey};
use sha2::Sha256;
use thiserror::Error;
use zeroize::Zeroize;

const NONCE_LEN: usize = 12;
const COMPRESSED_KEY_LEN: usize = 33;
const TAG_LEN: usize = 16;

#[derive(Debug, Error)]
pub enum KasiaCipherError {
    #[error("invalid secp256k1 key")]
    InvalidKey,
    #[error("invalid Kasia ciphertext")]
    InvalidCiphertext,
    #[error("Kasia key derivation failed")]
    Kdf,
    #[error("Kasia authenticated encryption failed")]
    Aead,
}

pub fn public_key_from_secret(
    secret: &[u8; 32],
) -> Result<[u8; COMPRESSED_KEY_LEN], KasiaCipherError> {
    let secret = SecretKey::from_slice(secret).map_err(|_| KasiaCipherError::InvalidKey)?;
    Ok(PublicKey::from_secret_key(&Secp256k1::new(), &secret).serialize())
}

/// KaChat-compatible ECIES frame:
/// ephemeral compressed secp256k1 key || nonce || ChaCha20-Poly1305 ciphertext+tag.
pub fn encrypt_for(
    recipient: &[u8; COMPRESSED_KEY_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>, KasiaCipherError> {
    let recipient = PublicKey::from_slice(recipient).map_err(|_| KasiaCipherError::InvalidKey)?;
    let ephemeral = SecretKey::new(&mut OsRng);
    let ephemeral_public = PublicKey::from_secret_key(&Secp256k1::new(), &ephemeral).serialize();
    let mut key = derive_key(&ecdh_x_coordinate(&recipient, &ephemeral))?;
    let cipher = ChaCha20Poly1305::new((&key).into());
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| KasiaCipherError::Aead)?;
    key.zeroize();
    let mut out = Vec::with_capacity(COMPRESSED_KEY_LEN + NONCE_LEN + sealed.len());
    out.extend_from_slice(&ephemeral_public);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

pub fn decrypt_for(secret: &[u8; 32], encoded: &[u8]) -> Result<Vec<u8>, KasiaCipherError> {
    if encoded.len() <= NONCE_LEN + COMPRESSED_KEY_LEN + TAG_LEN {
        return Err(KasiaCipherError::InvalidCiphertext);
    }
    let key_end = COMPRESSED_KEY_LEN;
    let nonce_end = key_end + NONCE_LEN;
    let ephemeral =
        PublicKey::from_slice(&encoded[..key_end]).map_err(|_| KasiaCipherError::InvalidKey)?;
    let secret = SecretKey::from_slice(secret).map_err(|_| KasiaCipherError::InvalidKey)?;
    let mut key = derive_key(&ecdh_x_coordinate(&ephemeral, &secret))?;
    let cipher = ChaCha20Poly1305::new((&key).into());
    let nonce = Nonce::from_slice(&encoded[key_end..nonce_end]);
    let opened = cipher
        .decrypt(nonce, &encoded[nonce_end..])
        .map_err(|_| KasiaCipherError::Aead);
    key.zeroize();
    opened
}

fn ecdh_x_coordinate(public: &PublicKey, secret: &SecretKey) -> [u8; 32] {
    let point = shared_secret_point(public, secret);
    let mut x = [0u8; 32];
    x.copy_from_slice(&point[..32]);
    x
}

fn derive_key(shared_x: &[u8; 32]) -> Result<[u8; 32], KasiaCipherError> {
    let hk = Hkdf::<Sha256>::new(None, shared_x);
    let mut key = [0u8; 32];
    hk.expand(b"", &mut key)
        .map_err(|_| KasiaCipherError::Kdf)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecies_round_trip_uses_kachat_frame_layout() {
        let secret = [7u8; 32];
        let public = public_key_from_secret(&secret).unwrap();
        let sealed = encrypt_for(&public, b"hello KaChat").unwrap();
        assert_eq!(sealed.len(), NONCE_LEN + COMPRESSED_KEY_LEN + 12 + TAG_LEN);
        assert!(matches!(sealed[0], 0x02 | 0x03));
        assert_eq!(decrypt_for(&secret, &sealed).unwrap(), b"hello KaChat");
    }

    #[test]
    fn authenticated_frame_rejects_tampering_and_truncation() {
        let secret = [9u8; 32];
        let public = public_key_from_secret(&secret).unwrap();
        let mut sealed = encrypt_for(&public, b"integrity").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(decrypt_for(&secret, &sealed).is_err());
        assert!(decrypt_for(&secret, &[0u8; COMPRESSED_KEY_LEN + NONCE_LEN + TAG_LEN]).is_err());
    }
}
