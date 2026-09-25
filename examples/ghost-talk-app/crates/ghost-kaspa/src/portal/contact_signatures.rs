use super::carriers::{
    gtcd_digest, GTCA_SIGNATURE_DOMAIN, GTCR_SIGNATURE_DOMAIN, KKTP_CALL_SIGNAL_SIGNATURE_DOMAIN,
    KKTP_DISCOVERY_SIGNATURE_DOMAIN, KKTP_RESPONSE_SIGNATURE_DOMAIN,
    KKTP_SESSION_END_SIGNATURE_DOMAIN,
};
#[cfg(feature = "upstream")]
pub fn p2pk_compressed_public_key(address: &str) -> Result<[u8; 33], String> {
    let (version, pubkey_x) = kaspa_portal::primitives::address::decode_address(address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("Kasia peer must use a Kaspa P2PK address".into());
    }
    if pubkey_x.len() != 32 {
        return Err("Kaspa P2PK address must contain a 32-byte x-only key".into());
    }
    let mut compressed = [0u8; 33];
    compressed[0] = 0x02;
    compressed[1..].copy_from_slice(&pubkey_x);
    Ok(compressed)
}

#[cfg(feature = "upstream")]
fn verify_p2pk_schnorr(
    address: &str,
    signature_hex: &str,
    digest: &[u8; 32],
    address_error: &str,
    signature_label: &str,
) -> Result<(), String> {
    let (version, pubkey_x) = kaspa_portal::primitives::address::decode_address(address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err(address_error.to_string());
    }
    let signature = hex::decode(signature_hex)
        .map_err(|_| format!("{signature_label} signature is not hex"))?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| format!("{signature_label} signature must be exactly 64 bytes"))?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(&pubkey_x, digest, &signature)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "upstream")]
pub fn sign_domain_message(
    private_key: &[u8; 32],
    domain: &[u8],
    message: &[u8],
) -> Result<String, String> {
    let digest = domain_message_digest(domain, message)?;
    let signature = kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &digest)
        .map_err(|error| error.to_string())?;
    Ok(hex::encode(signature.bytes))
}

#[cfg(feature = "upstream")]
pub fn verify_domain_message(
    address: &str,
    signature_hex: &str,
    domain: &[u8],
    message: &[u8],
) -> Result<(), String> {
    verify_p2pk_schnorr(
        address,
        signature_hex,
        &domain_message_digest(domain, message)?,
        "domain signer must use a Kaspa P2PK address",
        "domain message",
    )
}

#[cfg(feature = "upstream")]
fn domain_message_digest(domain: &[u8], message: &[u8]) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    if domain.is_empty() || domain.len() > 96 {
        return Err("signature domain length is invalid".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/DomainSignature/v1\0");
    hasher.update((domain.len() as u16).to_le_bytes());
    hasher.update(domain);
    hasher.update(message);
    Ok(hasher.finalize().into())
}

/// Verify that a GTCD is signed by the x-only BIP340 key encoded by its Kaspa
/// P2PK address.  P2SH/ECDSA address forms are rejected for descriptors.
#[cfg(feature = "upstream")]
pub fn verify_gtcd(descriptor: &ghost_protocol::GhostContactDescriptor) -> Result<(), String> {
    verify_p2pk_schnorr(
        &descriptor.kaspa_address,
        &descriptor.signature_hex,
        &gtcd_digest(descriptor)?,
        "GTCD owner must be a Kaspa P2PK address",
        "GTCD",
    )
}

/// Sign a descriptor with a Kaspa P2PK private key, then verify the result
/// against the descriptor address so a mismatched account key fails closed.
#[cfg(feature = "upstream")]
pub fn sign_gtcd(
    descriptor: &mut ghost_protocol::GhostContactDescriptor,
    private_key: &[u8; 32],
) -> Result<(), String> {
    descriptor.signature_hex.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &gtcd_digest(descriptor)?)
            .map_err(|e| e.to_string())?;
    descriptor.signature_hex = hex::encode(signature.bytes);
    verify_gtcd(descriptor)
}

#[cfg(feature = "upstream")]
pub(crate) fn contact_request_digest(
    request: &ghost_protocol::GhostContactRequest,
) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = request.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(if request.version == ghost_protocol::GHOST_KKTP_VERSION {
        KKTP_DISCOVERY_SIGNATURE_DOMAIN
    } else {
        GTCR_SIGNATURE_DOMAIN
    });
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

#[cfg(feature = "upstream")]
pub fn verify_contact_request(request: &ghost_protocol::GhostContactRequest) -> Result<(), String> {
    verify_gtcd(&request.sender)?;
    verify_p2pk_schnorr(
        &request.sender.kaspa_address,
        &request.signature_hex,
        &contact_request_digest(request)?,
        "contact-request signer must be a Kaspa P2PK address",
        "contact-request",
    )
}

#[cfg(feature = "upstream")]
pub fn sign_contact_request(
    request: &mut ghost_protocol::GhostContactRequest,
    private_key: &[u8; 32],
) -> Result<(), String> {
    request.signature_hex.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &contact_request_digest(request)?)
            .map_err(|e| e.to_string())?;
    request.signature_hex = hex::encode(signature.bytes);
    verify_contact_request(request)
}

#[cfg(feature = "upstream")]
pub(crate) fn contact_accept_digest(
    accepted: &ghost_protocol::GhostContactAccept,
) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = accepted.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(if accepted.version == ghost_protocol::GHOST_KKTP_VERSION {
        KKTP_RESPONSE_SIGNATURE_DOMAIN
    } else {
        GTCA_SIGNATURE_DOMAIN
    });
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

#[cfg(feature = "upstream")]
pub fn verify_contact_accept(accepted: &ghost_protocol::GhostContactAccept) -> Result<(), String> {
    verify_gtcd(&accepted.responder)?;
    verify_p2pk_schnorr(
        &accepted.acceptor_kaspa_address,
        &accepted.signature_hex,
        &contact_accept_digest(accepted)?,
        "contact-accept signer must be a Kaspa P2PK address",
        "contact-accept",
    )
}

#[cfg(feature = "upstream")]
pub fn sign_contact_accept(
    accepted: &mut ghost_protocol::GhostContactAccept,
    private_key: &[u8; 32],
) -> Result<(), String> {
    accepted.signature_hex.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &contact_accept_digest(accepted)?)
            .map_err(|e| e.to_string())?;
    accepted.signature_hex = hex::encode(signature.bytes);
    verify_contact_accept(accepted)
}

#[cfg(feature = "upstream")]
pub(crate) fn call_signal_digest(
    signal: &ghost_protocol::GhostCallSignal,
) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = signal.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(KKTP_CALL_SIGNAL_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

/// Verify a standalone call-control signal against the stable Kaspa P2PK key
/// carried by its signed Ghost descriptor. Call signaling deliberately does not
/// depend on an active HYDRA/KKTP chat ratchet.
#[cfg(feature = "upstream")]
pub fn verify_call_signal(signal: &ghost_protocol::GhostCallSignal) -> Result<(), String> {
    verify_gtcd(&signal.sender)?;
    verify_p2pk_schnorr(
        &signal.sender.kaspa_address,
        &signal.signature_hex,
        &call_signal_digest(signal)?,
        "call-signal signer must be a Kaspa P2PK address",
        "call-signal",
    )
}

#[cfg(feature = "upstream")]
pub fn sign_call_signal(
    signal: &mut ghost_protocol::GhostCallSignal,
    private_key: &[u8; 32],
) -> Result<(), String> {
    signal.signature_hex.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &call_signal_digest(signal)?)
            .map_err(|e| e.to_string())?;
    signal.signature_hex = hex::encode(signature.bytes);
    verify_call_signal(signal)
}

#[cfg(feature = "upstream")]
pub(crate) fn kktp_session_end_digest(
    end: &ghost_protocol::KktpSessionEnd,
) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = end.kaspa_signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(KKTP_SESSION_END_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

#[cfg(feature = "upstream")]
pub fn verify_kktp_session_end(end: &ghost_protocol::KktpSessionEnd) -> Result<(), String> {
    ghost_kaspa_validate_peer_address(&end.recipient_kaspa_address)?;
    verify_p2pk_schnorr(
        &end.sender_kaspa_address,
        &end.sig,
        &kktp_session_end_digest(end)?,
        "KKTP session_end signer must be a Kaspa P2PK address",
        "KKTP session_end Kaspa",
    )
}

#[cfg(feature = "upstream")]
pub fn sign_kktp_session_end(
    end: &mut ghost_protocol::KktpSessionEnd,
    private_key: &[u8; 32],
) -> Result<(), String> {
    end.sig.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &kktp_session_end_digest(end)?)
            .map_err(|e| e.to_string())?;
    end.sig = hex::encode(signature.bytes);
    verify_kktp_session_end(end)
}

#[cfg(feature = "upstream")]
pub(crate) fn ghost_kaspa_validate_peer_address(address: &str) -> Result<(), String> {
    let (version, _) = kaspa_portal::primitives::address::decode_address(address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("KKTP session_end recipient must be a Kaspa P2PK address".into());
    }
    Ok(())
}

mod delivery_ack;
pub use delivery_ack::{sign_delivery_ack, verify_delivery_ack};
