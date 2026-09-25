use super::super::carriers::GTACK_SIGNATURE_DOMAIN;

fn delivery_ack_digest(ack: &ghost_protocol::GhostDeliveryAck) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = ack.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(GTACK_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

/// Verify a restart-safe delivery acknowledgement against its stable Kaspa P2PK address.
pub fn verify_delivery_ack(ack: &ghost_protocol::GhostDeliveryAck) -> Result<(), String> {
    ensure(
        ack.version == 1,
        "unsupported Ghost Talk delivery acknowledgement version",
    )?;
    ensure(
        ack.message_id.len() == 32 && ack.message_id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Ghost Talk delivery acknowledgement message id is invalid",
    )?;
    ensure(
        ack.signer_hydra_id.len() == 64
            && ack
                .signer_hydra_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "Ghost Talk delivery acknowledgement HYDRA identity is invalid",
    )?;
    ensure(
        ack.destination_hydra_id.len() == 64
            && ack
                .destination_hydra_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "Ghost Talk delivery acknowledgement HYDRA identity is invalid",
    )?;
    let (version, pubkey_x) =
        kaspa_portal::primitives::address::decode_address(&ack.signer_kaspa_address)?;
    ensure(
        version == kaspa_portal::primitives::address::AddressType::P2pk as u8,
        "delivery acknowledgement signer must be a Kaspa P2PK address",
    )?;
    let signature = hex::decode(&ack.signature_hex)
        .map_err(|_| "delivery acknowledgement signature is not hex".to_string())?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| "delivery acknowledgement signature must be exactly 64 bytes".to_string())?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(&pubkey_x, &delivery_ack_digest(ack)?, &signature)
        .map_err(|error| error.to_string())
}

fn ensure(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

/// Sign a delivery acknowledgement and verify it against the encoded signer address.
pub fn sign_delivery_ack(
    ack: &mut ghost_protocol::GhostDeliveryAck,
    private_key: &[u8; 32],
) -> Result<(), String> {
    ack.signature_hex.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &delivery_ack_digest(ack)?)
            .map_err(|error| error.to_string())?;
    ack.signature_hex = hex::encode(signature.bytes);
    verify_delivery_ack(ack)
}
