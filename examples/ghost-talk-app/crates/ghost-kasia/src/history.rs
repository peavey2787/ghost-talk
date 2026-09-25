use crate::KasiaHandshake;
#[cfg(feature = "native")]
use crate::{decrypt_for, KasiaContextualMessageResponse, KasiaHandshakeResponse};
#[cfg(feature = "native")]
use base64::Engine;
use serde::{Deserialize, Serialize};
#[cfg(feature = "native")]
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KasiaMessage {
    pub tx_id: String,
    pub sender_address: String,
    pub alias: String,
    pub text: String,
    pub block_time: u64,
    pub outgoing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KasiaReceivedHandshake {
    pub tx_id: String,
    pub sender_address: String,
    pub receiver_address: String,
    pub block_time: u64,
    pub handshake: KasiaHandshake,
}

#[cfg(feature = "native")]
#[derive(Clone, Debug, Default)]
pub struct KasiaConversationHistory {
    seen: BTreeSet<String>,
    messages: Vec<KasiaMessage>,
}

#[cfg(feature = "native")]
impl KasiaConversationHistory {
    pub fn messages(&self) -> &[KasiaMessage] {
        &self.messages
    }

    pub fn ingest(
        &mut self,
        tx: KasiaContextualMessageResponse,
        secret: &[u8; 32],
        outgoing: bool,
    ) -> Result<bool, String> {
        if self.seen.contains(&tx.tx_id) {
            return Ok(false);
        }
        let ciphertext = decode_indexed_contextual(tx.message_payload.as_deref())?;
        let plaintext = decrypt_for(secret, &ciphertext).map_err(|error| error.to_string())?;
        let text =
            String::from_utf8(plaintext).map_err(|_| "Kasia message is not UTF-8".to_string())?;
        self.seen.insert(tx.tx_id.clone());
        self.messages.push(KasiaMessage {
            tx_id: tx.tx_id,
            sender_address: tx.sender,
            alias: tx.alias,
            text,
            block_time: tx.block_time.unwrap_or_default(),
            outgoing,
        });
        self.messages.sort_by_key(|message| message.block_time);
        Ok(true)
    }
}

#[cfg(feature = "native")]
pub fn decrypt_indexed_handshake(
    response: KasiaHandshakeResponse,
    secret: &[u8; 32],
) -> Result<KasiaReceivedHandshake, String> {
    let payload = response
        .message_payload
        .as_deref()
        .ok_or_else(|| "Kasia handshake has no message_payload".to_string())?;
    let ciphertext = hex::decode(payload)
        .map_err(|error| format!("Kasia handshake payload is not hex: {error}"))?;
    let plaintext = decrypt_for(secret, &ciphertext).map_err(|error| error.to_string())?;
    let handshake = KasiaHandshake::decode(&plaintext)?;
    Ok(KasiaReceivedHandshake {
        tx_id: response.tx_id,
        sender_address: response.sender,
        receiver_address: response.receiver,
        block_time: response.block_time.unwrap_or_default(),
        handshake,
    })
}

#[cfg(feature = "native")]
fn decode_indexed_contextual(payload: Option<&str>) -> Result<Vec<u8>, String> {
    let payload =
        payload.ok_or_else(|| "Kasia contextual message has no message_payload".to_string())?;
    let base64_bytes = hex::decode(payload)
        .map_err(|error| format!("Kasia contextual message payload is not hex: {error}"))?;
    let base64_text = std::str::from_utf8(&base64_bytes)
        .map_err(|_| "Kasia contextual message payload is not UTF-8 base64".to_string())?;
    base64::engine::general_purpose::STANDARD
        .decode(base64_text)
        .map_err(|error| format!("Kasia contextual message payload is not base64: {error}"))
}
