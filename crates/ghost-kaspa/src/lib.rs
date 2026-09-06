#![forbid(unsafe_code)]

use ghost_core::KaspaAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Outpoint {
    pub txid: String,
    pub index: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UtxoReservations {
    reserved: BTreeMap<Outpoint, String>,
}

impl UtxoReservations {
    pub fn reserve<I: IntoIterator<Item = Outpoint>>(
        &mut self,
        id: &str,
        items: I,
    ) -> Result<(), String> {
        let items: Vec<_> = items.into_iter().collect();
        if items.iter().any(|x| self.reserved.contains_key(x)) {
            return Err("UTXO already reserved".into());
        }
        for x in items {
            self.reserved.insert(x, id.into());
        }
        Ok(())
    }

    pub fn release(&mut self, id: &str) {
        self.reserved.retain(|_, v| v != id)
    }

    pub fn is_reserved(&self, o: &Outpoint) -> bool {
        self.reserved.contains_key(o)
    }
}

#[derive(Clone, Debug)]
pub struct LiveTransactionObservation {
    pub txid: String,
    pub daa_score: u64,
    pub payload: Vec<u8>,
}

#[cfg(feature = "upstream")]
pub mod wallet;

#[cfg(feature = "upstream")]
pub mod upstream {
    pub use kaspa_portal as portal;
}

pub fn validate_destination(s: &str) -> Result<KaspaAddress, String> {
    KaspaAddress::parse(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_are_atomic() {
        let o = Outpoint {
            txid: "a".into(),
            index: 0,
        };
        let mut r = UtxoReservations::default();
        r.reserve("a", [o.clone()]).unwrap();
        assert!(r.reserve("b", [o]).is_err())
    }
}

#[cfg(feature = "upstream")]
pub type PortalMassAnalysis = kaspa_portal::transaction::mass::TransactionAnalysis;

#[cfg(feature = "upstream")]
#[derive(Clone, Debug)]
pub struct PortalCurrentUtxo {
    pub amount: u64,
    pub script_public_key: Vec<u8>,
}

#[cfg(feature = "upstream")]
#[derive(Clone)]
pub struct PortalFacade {
    portal: kaspa_portal::KaspaPortal,
}

#[cfg(feature = "upstream")]
impl PortalFacade {
    /// Connect through the published Kaspa Portal 1.0.1 API. Native Portal
    /// futures are Send-safe and the SDK owns the persistent wRPC transport,
    /// reconnect/replay behavior, transaction planning, and notification mux.
    pub async fn connect(network: &str, endpoint: &str) -> Result<Self, String> {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            return Err("Kaspa Portal connection requires a concrete wRPC endpoint".into());
        }
        let network = kaspa_portal::primitives::NetworkId::parse(network)?;
        let portal = kaspa_portal::KaspaPortal::builder()
            .network(network)
            .endpoint(endpoint)
            .connect()
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self { portal })
    }

    pub fn endpoint(&self) -> Result<String, String> {
        self.portal
            .network()
            .map(|network| network.endpoint().to_owned())
            .map_err(|error| error.to_string())
    }

    /// Current spendable outputs from the selected public Kaspa node through
    /// Kaspa Portal. This is live wallet state, never REST.
    pub async fn current_utxos(
        &self,
        addresses: &[String],
    ) -> Result<Vec<PortalCurrentUtxo>, String> {
        self.portal
            .chain()
            .map_err(|error| error.to_string())?
            .utxos_many(addresses)
            .await
            .map_err(|error| error.to_string())
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| PortalCurrentUtxo {
                        amount: entry.amount,
                        script_public_key: entry.script_public_key,
                    })
                    .collect()
            })
    }

    /// Current virtual DAA score from the selected public Kaspa node through
    /// Kaspa Portal's shared persistent connection.
    pub async fn current_virtual_daa_score(&self) -> Result<u64, String> {
        self.portal
            .chain()
            .map_err(|error| error.to_string())?
            .virtual_daa_score()
            .await
            .map(kaspa_portal::primitives::DaaScore::get)
            .map_err(|error| error.to_string())
    }

    pub fn script_pubkey_for_address(address: &str) -> Result<Vec<u8>, String> {
        kaspa_portal::primitives::address::address_to_script_pubkey(address)
    }

    /// Use Portal 1.0.1's payload-aware planner for both payload and ordinary
    /// sends. Passing an empty payload preserves normal KAS-send semantics while
    /// still using the new mass/fee-aware UTXO selection.
    pub(crate) async fn plan_send_with_payload(
        &self,
        wallet: kaspa_portal::wallet::account::derivation::WalletData,
        destination: &str,
        amount_sompi: u64,
        requested_fee_sompi: u64,
        payload: &[u8],
    ) -> Result<String, String> {
        self.portal
            .transaction()
            .plan_send_with_payload(
                &wallet,
                destination,
                amount_sompi,
                requested_fee_sompi,
                payload,
            )
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn plan_consolidation(
        &self,
        wallet: kaspa_portal::wallet::account::derivation::WalletData,
        requested_fee_sompi: u64,
    ) -> Result<String, String> {
        self.portal
            .transaction()
            .plan_consolidation(&wallet, requested_fee_sompi)
            .await
            .map_err(|error| error.to_string())
    }

    /// Exact signed/finalizable transaction analysis using Portal 1.0.1's
    /// node-aware normal fee estimate and central mass policy.
    pub(crate) async fn analyze(&self, signed_pskb: &str) -> Result<PortalMassAnalysis, String> {
        self.portal
            .transaction()
            .analyze(signed_pskb)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn broadcast_signed_pskb(&self, signed_pskb: &str) -> Result<String, String> {
        let tx = self.portal.transaction();
        let consensus = tx.finalize(signed_pskb).map_err(|error| error.to_string())?;
        tx.broadcast(&consensus)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn subscribe_block_added(&self) -> Result<(), String> {
        self.portal
            .network()
            .map_err(|error| error.to_string())?
            .subscribe_block_added()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn next_block_added(&self) -> Result<LiveBlockEvent, String> {
        let block = self
            .portal
            .network()
            .map_err(|error| error.to_string())?
            .next_block_added()
            .await
            .map_err(|error| error.to_string())?;

        let mut observations = Vec::new();
        for (tx_index, transaction) in block.transactions.into_iter().enumerate() {
            if !is_ghost_carrier(&transaction.payload) {
                continue;
            }
            let txid = transaction.transaction_id.unwrap_or_else(|| {
                live_event_id(&block.block_hash, tx_index, &transaction.payload)
            });
            observations.push(LiveTransactionObservation {
                txid,
                daa_score: block.daa_score,
                payload: transaction.payload,
            });
        }
        Ok(LiveBlockEvent {
            block_hash: block.block_hash,
            daa_score: block.daa_score,
            observations,
        })
    }
}

#[cfg(all(test, feature = "upstream"))]
mod portal_send_contract_tests {
    use super::*;

    fn require_send<T: Send>(_: T) {}
    fn require_send_sync<T: Send + Sync>() {}

    #[test]
    fn facade_handle_and_connect_future_are_send_safe() {
        require_send_sync::<PortalFacade>();
        require_send(PortalFacade::connect("mainnet", "wss://example.invalid"));
    }

    #[allow(dead_code)]
    fn public_portal_futures_remain_send<'a>(
        portal: &'a PortalFacade,
        addresses: &'a [String],
        wallet: kaspa_portal::wallet::account::derivation::WalletData,
    ) {
        require_send(portal.current_utxos(addresses));
        require_send(portal.current_virtual_daa_score());
        require_send(portal.plan_send_with_payload(
            wallet,
            "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
            1,
            1,
            b"x",
        ));
    }
}

#[cfg(feature = "upstream")]
#[derive(Clone, Debug)]
pub struct LiveBlockEvent {
    pub block_hash: String,
    pub daa_score: u64,
    pub observations: Vec<LiveTransactionObservation>,
}

#[cfg(feature = "upstream")]
fn live_event_id(block_hash: &str, tx_index: usize, payload: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    // Block notifications normally include transaction verbose data. If a node
    // omits it, keep the live carrier rather than filtering it: derive a stable
    // event identifier from immutable block position + payload. Protocol
    // SID/sequence replay checks remain authoritative when history later supplies
    // the canonical transaction id.
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/live-block-event/v1\0");
    hasher.update(block_hash.as_bytes());
    hasher.update((tx_index as u64).to_le_bytes());
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

#[cfg(feature = "upstream")]
fn is_ghost_carrier(payload: &[u8]) -> bool {
    payload.starts_with(b"KKTP:")
        || (payload.len() >= 4
            && matches!(
                &payload[..4],
                b"GHST" | b"GTCD" | b"GTCR" | b"GTCA" | b"GTAK" | b"GTVA" | b"GTVL" | b"GTBK"
            ))
}

#[cfg(feature = "upstream")]
const GTCD_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTCD/v1\0";
#[cfg(feature = "upstream")]
const GTACK_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTAK/v1\0";
#[cfg(feature = "upstream")]
const GTCR_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTCR/v1\0";
#[cfg(feature = "upstream")]
const GTCA_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTCA/v1\0";
#[cfg(feature = "upstream")]
const KKTP_DISCOVERY_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/discovery/v2\0";
#[cfg(feature = "upstream")]
const KKTP_RESPONSE_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/response/v2\0";
#[cfg(feature = "upstream")]
const KKTP_SESSION_END_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/session-end/v2\0";

#[cfg(feature = "upstream")]
fn gtcd_digest(descriptor: &ghost_protocol::GhostContactDescriptor) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = descriptor.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(GTCD_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

/// Compute the exact human-readable message digest used by KasSigner firmware's
/// Sign Message workflow. Keeping this helper in the Kaspa owner lets Ghost Talk
/// verify a hardware proof-of-possession without importing signer private-key code.
#[cfg(feature = "upstream")]
pub fn kassigner_message_digest(message: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    const DOMAIN: &[u8] = b"KasSigner Signed Message v1\0";
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update((message.len() as u64).to_le_bytes());
    hasher.update(message);
    hasher.finalize().into()
}

/// Verify a KasSigner Sign Message result against an exact x-only account public key.
#[cfg(feature = "upstream")]
pub fn verify_kassigner_message_signature(
    pubkey_x: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), String> {
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes: *signature };
    kaspa_portal::crypto::schnorr::schnorr_verify(
        pubkey_x,
        &kassigner_message_digest(message),
        &signature,
    )
    .map_err(|error| error.to_string())
}

/// Verify that a GTCD is signed by the x-only BIP340 key encoded by its Kaspa
/// P2PK address.  P2SH/ECDSA address forms are rejected for descriptors.
#[cfg(feature = "upstream")]
pub fn verify_gtcd(descriptor: &ghost_protocol::GhostContactDescriptor) -> Result<(), String> {
    let (version, pubkey_x) =
        kaspa_portal::primitives::address::decode_address(&descriptor.kaspa_address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("GTCD owner must be a Kaspa P2PK address".into());
    }
    let signature = hex::decode(&descriptor.signature_hex)
        .map_err(|_| "GTCD signature is not hex".to_string())?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| "GTCD signature must be exactly 64 bytes".to_string())?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(&pubkey_x, &gtcd_digest(descriptor)?, &signature)
        .map_err(|e| e.to_string())
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
fn contact_request_digest(request: &ghost_protocol::GhostContactRequest) -> Result<[u8; 32], String> {
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
    let (version, pubkey_x) =
        kaspa_portal::primitives::address::decode_address(&request.sender.kaspa_address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("contact-request signer must be a Kaspa P2PK address".into());
    }
    let signature = hex::decode(&request.signature_hex)
        .map_err(|_| "contact-request signature is not hex".to_string())?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| "contact-request signature must be exactly 64 bytes".to_string())?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(&pubkey_x, &contact_request_digest(request)?, &signature)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "upstream")]
pub fn sign_contact_request(
    request: &mut ghost_protocol::GhostContactRequest,
    private_key: &[u8; 32],
) -> Result<(), String> {
    request.signature_hex.clear();
    let signature = kaspa_portal::crypto::schnorr::schnorr_sign(
        private_key,
        &contact_request_digest(request)?,
    )
    .map_err(|e| e.to_string())?;
    request.signature_hex = hex::encode(signature.bytes);
    verify_contact_request(request)
}

#[cfg(feature = "upstream")]
fn contact_accept_digest(accepted: &ghost_protocol::GhostContactAccept) -> Result<[u8; 32], String> {
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
    let (version, pubkey_x) =
        kaspa_portal::primitives::address::decode_address(&accepted.acceptor_kaspa_address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("contact-accept signer must be a Kaspa P2PK address".into());
    }
    let signature = hex::decode(&accepted.signature_hex)
        .map_err(|_| "contact-accept signature is not hex".to_string())?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| "contact-accept signature must be exactly 64 bytes".to_string())?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(&pubkey_x, &contact_accept_digest(accepted)?, &signature)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "upstream")]
pub fn sign_contact_accept(
    accepted: &mut ghost_protocol::GhostContactAccept,
    private_key: &[u8; 32],
) -> Result<(), String> {
    accepted.signature_hex.clear();
    let signature = kaspa_portal::crypto::schnorr::schnorr_sign(
        private_key,
        &contact_accept_digest(accepted)?,
    )
    .map_err(|e| e.to_string())?;
    accepted.signature_hex = hex::encode(signature.bytes);
    verify_contact_accept(accepted)
}

#[cfg(feature = "upstream")]
fn kktp_session_end_digest(end: &ghost_protocol::KktpSessionEnd) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = end.kaspa_signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(KKTP_SESSION_END_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

#[cfg(feature = "upstream")]
pub fn verify_kktp_session_end(end: &ghost_protocol::KktpSessionEnd) -> Result<(), String> {
    let (version, pubkey_x) =
        kaspa_portal::primitives::address::decode_address(&end.sender_kaspa_address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("KKTP session_end signer must be a Kaspa P2PK address".into());
    }
    ghost_kaspa_validate_peer_address(&end.recipient_kaspa_address)?;
    let signature = hex::decode(&end.sig)
        .map_err(|_| "KKTP session_end Kaspa signature is not hex".to_string())?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| "KKTP session_end Kaspa signature must be exactly 64 bytes".to_string())?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(
        &pubkey_x,
        &kktp_session_end_digest(end)?,
        &signature,
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "upstream")]
pub fn sign_kktp_session_end(
    end: &mut ghost_protocol::KktpSessionEnd,
    private_key: &[u8; 32],
) -> Result<(), String> {
    end.sig.clear();
    let signature = kaspa_portal::crypto::schnorr::schnorr_sign(
        private_key,
        &kktp_session_end_digest(end)?,
    )
    .map_err(|e| e.to_string())?;
    end.sig = hex::encode(signature.bytes);
    verify_kktp_session_end(end)
}

#[cfg(feature = "upstream")]
fn ghost_kaspa_validate_peer_address(address: &str) -> Result<(), String> {
    let (version, _) = kaspa_portal::primitives::address::decode_address(address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("KKTP session_end recipient must be a Kaspa P2PK address".into());
    }
    Ok(())
}

#[cfg(feature = "upstream")]
fn delivery_ack_digest(ack: &ghost_protocol::GhostDeliveryAck) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = ack.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(GTACK_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}

/// Verify a restart-safe Ghost Talk delivery acknowledgement against the
/// stable Kaspa P2PK address that signed it.
#[cfg(feature = "upstream")]
pub fn verify_delivery_ack(ack: &ghost_protocol::GhostDeliveryAck) -> Result<(), String> {
    if ack.version != 1 {
        return Err("unsupported Ghost Talk delivery acknowledgement version".into());
    }
    if ack.message_id.len() != 32 || !ack.message_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk delivery acknowledgement message id is invalid".into());
    }
    if ack.signer_hydra_id.len() != 64
        || !ack.signer_hydra_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || ack.destination_hydra_id.len() != 64
        || !ack.destination_hydra_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Ghost Talk delivery acknowledgement HYDRA identity is invalid".into());
    }
    let (version, pubkey_x) =
        kaspa_portal::primitives::address::decode_address(&ack.signer_kaspa_address)?;
    if version != kaspa_portal::primitives::address::AddressType::P2pk as u8 {
        return Err("delivery acknowledgement signer must be a Kaspa P2PK address".into());
    }
    let signature = hex::decode(&ack.signature_hex)
        .map_err(|_| "delivery acknowledgement signature is not hex".to_string())?;
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| "delivery acknowledgement signature must be exactly 64 bytes".to_string())?;
    let signature = kaspa_portal::crypto::schnorr::SchnorrSignature { bytes };
    kaspa_portal::crypto::schnorr::schnorr_verify(&pubkey_x, &delivery_ack_digest(ack)?, &signature)
        .map_err(|e| e.to_string())
}

/// Sign a Ghost Talk delivery acknowledgement with the stable receive-address
/// private key, then verify it against the encoded signer address.
#[cfg(feature = "upstream")]
pub fn sign_delivery_ack(
    ack: &mut ghost_protocol::GhostDeliveryAck,
    private_key: &[u8; 32],
) -> Result<(), String> {
    ack.signature_hex.clear();
    let signature =
        kaspa_portal::crypto::schnorr::schnorr_sign(private_key, &delivery_ack_digest(ack)?)
            .map_err(|e| e.to_string())?;
    ack.signature_hex = hex::encode(signature.bytes);
    verify_delivery_ack(ack)
}


#[cfg(all(test, feature = "upstream"))]
mod contact_bootstrap_tests {
    use super::*;

    fn address_for(private_key: &[u8; 32]) -> String {
        let xonly = kaspa_portal::wallet::derivation::bip32::pubkey_from_raw_key(private_key)
            .expect("valid test key");
        kaspa_portal::primitives::address::encode_p2pk_address(&xonly, "kaspatest")
    }

    fn signed_descriptor(private_key: &[u8; 32], hydra_byte: u8) -> ghost_protocol::GhostContactDescriptor {
        let mut descriptor = ghost_protocol::GhostContactDescriptor {
            version: 1,
            kaspa_address: address_for(private_key),
            hydra_contact_card_b64: "AQIDBA==".to_owned(),
            display_name: "test peer".to_owned(),
            hydra_identity_id: hex::encode([hydra_byte; 32]),
            discoverable: false,
            username: String::new(),
            description: String::new(),
            interests: Vec::new(),
            capabilities: vec!["hydra-v1".to_owned()],
            expires_daa: None,
            signature_hex: String::new(),
        };
        sign_gtcd(&mut descriptor, private_key).expect("sign GTCD");
        descriptor
    }

    #[test]
    fn kassigner_identity_proof_signature_binds_exact_challenge() {
        let private_key = [9u8; 32];
        let pubkey_x = kaspa_portal::wallet::derivation::bip32::pubkey_from_raw_key(&private_key)
            .expect("valid test key");
        let challenge = b"Ghost Talk Kaspa identity ownership proof v1\nNetwork: testnet-10\nHYDRA identity: test";
        let digest = kassigner_message_digest(challenge);
        let signature = kaspa_portal::crypto::schnorr::schnorr_sign(&private_key, &digest)
            .expect("sign proof");

        verify_kassigner_message_signature(&pubkey_x, challenge, &signature.bytes)
            .expect("verify exact proof");
        assert!(verify_kassigner_message_signature(
            &pubkey_x,
            b"Ghost Talk Kaspa identity ownership proof v1\nNetwork: mainnet\nHYDRA identity: test",
            &signature.bytes,
        )
        .is_err());
    }

    #[test]
    fn contact_request_signature_binds_recipient_destination() {
        let sender_key = [1u8; 32];
        let mut request = ghost_protocol::GhostContactRequest {
            version: 1,
            request_id: "12".repeat(16),
            recipient_kaspa_address: address_for(&[2u8; 32]),
            sender: signed_descriptor(&sender_key, 3),
            signature_hex: String::new(),
        };
        sign_contact_request(&mut request, &sender_key).expect("sign request");
        verify_contact_request(&request).expect("verify request");
        request.recipient_kaspa_address = address_for(&[4u8; 32]);
        assert!(verify_contact_request(&request).is_err());
    }

    #[test]
    fn kktp_session_end_signature_binds_sid_sender_and_destination() {
        let sender_key = [21u8; 32];
        let recipient_key = [22u8; 32];
        let sender_address = address_for(&sender_key);
        let recipient_address = address_for(&recipient_key);
        let mut end = ghost_protocol::KktpSessionEnd {
            kind: "session_end".into(),
            version: ghost_protocol::GHOST_KKTP_VERSION,
            sid: "11".repeat(16),
            initiator_hydra_id: "22".repeat(32),
            responder_hydra_id: "33".repeat(32),
            sender_hydra_id: "22".repeat(32),
            sender_kaspa_address: sender_address,
            recipient_kaspa_address: recipient_address,
            reason: "left".into(),
            pq_sig_b64: "AQID".into(),
            sig: String::new(),
        };
        sign_kktp_session_end(&mut end, &sender_key).expect("sign session_end");
        verify_kktp_session_end(&end).expect("verify session_end");

        let mut wrong_sid = end.clone();
        wrong_sid.sid = "44".repeat(16);
        assert!(verify_kktp_session_end(&wrong_sid).is_err());
        let mut wrong_destination = end.clone();
        wrong_destination.recipient_kaspa_address = address_for(&[23u8; 32]);
        assert!(verify_kktp_session_end(&wrong_destination).is_err());
    }

    #[test]
    fn contact_accept_signature_binds_exact_acceptor_and_return_destination() {
        let responder_key = [5u8; 32];
        let mut accepted = ghost_protocol::GhostContactAccept {
            version: 1,
            request_id: "34".repeat(16),
            recipient_kaspa_address: address_for(&[6u8; 32]),
            acceptor_kaspa_address: address_for(&responder_key),
            responder: signed_descriptor(&responder_key, 7),
            signature_hex: String::new(),
        };
        sign_contact_accept(&mut accepted, &responder_key).expect("sign acceptance");
        verify_contact_accept(&accepted).expect("verify acceptance");

        let mut wrong_acceptor = accepted.clone();
        wrong_acceptor.acceptor_kaspa_address = address_for(&[8u8; 32]);
        assert!(verify_contact_accept(&wrong_acceptor).is_err());

        let mut wrong_destination = accepted;
        wrong_destination.recipient_kaspa_address = address_for(&[9u8; 32]);
        assert!(verify_contact_accept(&wrong_destination).is_err());
    }
}
