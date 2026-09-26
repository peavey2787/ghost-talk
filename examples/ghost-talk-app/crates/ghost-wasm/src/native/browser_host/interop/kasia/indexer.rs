//! Kasia indexer history and received handshakes (archival reads).

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_kasia::{
    decrypt_for, KasiaHandshake, KasiaHistoryRequest, KasiaMessage, KasiaReceivedHandshake,
};
use ghost_kaspa::wallet::{self, WalletPublic};
use serde::Deserialize;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;
use zeroize::Zeroize;

use crate::native::browser_host::support::{
    debug,
    util::{open_wallet_secret, required, required_str, to_value},
};

const PAGE_LIMIT: usize = 50;
const MAX_PAGES: usize = 20;

#[derive(Clone, Debug, Deserialize)]
struct HandshakeResponse {
    tx_id: String,
    sender: String,
    receiver: String,
    #[serde(default)]
    block_time: Option<u64>,
    #[serde(default)]
    message_payload: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ContextResponse {
    tx_id: String,
    sender: String,
    alias: String,
    #[serde(default)]
    block_time: Option<u64>,
    #[serde(default)]
    message_payload: Option<String>,
}

pub(super) async fn history(args: &Value) -> Result<Value, String> {
    let request: KasiaHistoryRequest<WalletPublic> = required(args, "request")?;
    let their_alias = request
        .mapping
        .their_alias
        .as_deref()
        .ok_or("Kasia peer alias is not established")?;
    let secret = open_wallet_secret(&request.password, &request.sealed, &request.public)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let alias_hex = hex::encode(their_alias.as_bytes());
    let params = [
        ("address", request.mapping.kaspa_address.as_str()),
        ("alias", alias_hex.as_str()),
    ];
    let values: Vec<ContextResponse> = paginated(
        &request.indexer_url,
        "/contextual-messages/by-sender",
        &params,
        request.block_time,
    )
    .await?;
    let mut messages = Vec::new();
    for value in values {
        if let Ok(message) = decrypt_context(value, &identity_secret) {
            messages.push(message);
        }
    }
    identity_secret.zeroize();
    messages.sort_by_key(|message| message.block_time);
    to_value(messages)
}

pub(super) async fn received_handshakes(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let password = required_str(args, "password")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let public: WalletPublic = required(args, "public")?;
    let indexer_url = required_str(args, "indexerUrl")?;
    let block_time = args.get("blockTime").and_then(Value::as_u64).unwrap_or(0);
    let own = public
        .receive_addresses
        .first()
        .ok_or("wallet has no Kasia receiving address")?;
    let secret = open_wallet_secret(password, &sealed, &public)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let values: Vec<HandshakeResponse> = paginated(
        indexer_url,
        "/handshakes/by-receiver",
        &[("address", own.as_str())],
        block_time,
    )
    .await?;
    let handshakes = values
        .into_iter()
        .filter_map(|value| decrypt_handshake(value, &identity_secret).ok())
        .collect::<Vec<_>>();
    identity_secret.zeroize();
    debug::record(
        "debug",
        "kasia",
        "received-handshakes",
        format!("profile={profile_id} count={}", handshakes.len()),
    );
    to_value(handshakes)
}

fn decrypt_context(value: ContextResponse, secret: &[u8; 32]) -> Result<KasiaMessage, String> {
    let payload = value
        .message_payload
        .as_deref()
        .ok_or_else(|| "Kasia contextual message has no message_payload".to_string())?;
    let base64_bytes = hex::decode(payload)
        .map_err(|error| format!("Kasia contextual message payload is not hex: {error}"))?;
    let base64_text = std::str::from_utf8(&base64_bytes)
        .map_err(|_| "Kasia contextual message payload is not UTF-8 base64".to_string())?;
    let ciphertext = STANDARD
        .decode(base64_text)
        .map_err(|error| format!("Kasia contextual message payload is not base64: {error}"))?;
    let plaintext = decrypt_for(secret, &ciphertext).map_err(|error| error.to_string())?;
    let text =
        String::from_utf8(plaintext).map_err(|_| "Kasia message is not UTF-8".to_string())?;
    Ok(KasiaMessage {
        tx_id: value.tx_id,
        sender_address: value.sender,
        alias: value.alias,
        text,
        block_time: value.block_time.unwrap_or_default(),
        outgoing: false,
    })
}

fn decrypt_handshake(
    value: HandshakeResponse,
    secret: &[u8; 32],
) -> Result<KasiaReceivedHandshake, String> {
    let payload = value
        .message_payload
        .as_deref()
        .ok_or_else(|| "Kasia handshake has no message_payload".to_string())?;
    let ciphertext = hex::decode(payload)
        .map_err(|error| format!("Kasia handshake payload is not hex: {error}"))?;
    let plaintext = decrypt_for(secret, &ciphertext).map_err(|error| error.to_string())?;
    let handshake = KasiaHandshake::decode(&plaintext)?;
    Ok(KasiaReceivedHandshake {
        tx_id: value.tx_id,
        sender_address: value.sender,
        receiver_address: value.receiver,
        block_time: value.block_time.unwrap_or_default(),
        handshake,
    })
}

async fn paginated<T>(
    base: &str,
    endpoint: &str,
    params: &[(&str, &str)],
    start: u64,
) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de> + BlockTimed,
{
    validate_indexer(base)?;
    let mut cursor = start;
    let mut all = Vec::new();
    for _ in 0..MAX_PAGES {
        let mut url = format!(
            "{}{}?limit={PAGE_LIMIT}&block_time={cursor}",
            base.trim_end_matches('/'),
            endpoint
        );
        for (key, value) in params {
            url.push('&');
            url.push_str(key);
            url.push('=');
            url.push_str(&encode(value));
        }
        let page: Vec<T> = fetch_json(&url).await?;
        let count = page.len();
        let next = page
            .iter()
            .filter_map(BlockTimed::block_time)
            .max()
            .unwrap_or(cursor);
        all.extend(page);
        if count < PAGE_LIMIT || next <= cursor {
            break;
        }
        cursor = next;
    }
    Ok(all)
}

trait BlockTimed {
    fn block_time(&self) -> Option<u64>;
}
impl BlockTimed for HandshakeResponse {
    fn block_time(&self) -> Option<u64> {
        self.block_time
    }
}
impl BlockTimed for ContextResponse {
    fn block_time(&self) -> Option<u64> {
        self.block_time
    }
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let window = web_sys::window().ok_or("Browser window unavailable")?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(crate::native::invoke::js_error)?
        .dyn_into::<Response>()
        .map_err(|_| "Kasia indexer returned a non-HTTP response".to_string())?;
    if !response.ok() {
        return Err(format!("Kasia indexer returned HTTP {}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(crate::native::invoke::js_error)?)
        .await
        .map_err(crate::native::invoke::js_error)?
        .as_string()
        .ok_or("Kasia indexer response was not text")?;
    serde_json::from_str(&text).map_err(|error| format!("invalid Kasia indexer response: {error}"))
}

fn validate_indexer(base: &str) -> Result<(), String> {
    let base = base.trim();
    if base.starts_with("https://")
        || base.starts_with("http://localhost")
        || base.starts_with("http://127.0.0.1")
    {
        Ok(())
    } else {
        Err("Kasia indexer must use HTTPS (localhost is allowed for development)".into())
    }
}

fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}
