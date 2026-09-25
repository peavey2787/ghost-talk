use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_api::BroadcastResult;
use ghost_kasia::{
    decrypt_for, encode_comm, encode_handshake, encrypt_for, public_key_from_secret,
    KasiaHandshake, KasiaHistoryRequest, KasiaIdentityProjection, KasiaMessage,
    KasiaReceivedHandshake, KasiaSendRequest,
};
use ghost_kaspa::wallet::{self, WalletPublic, WalletSecret};
use serde::Deserialize;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;
use zeroize::Zeroize;

use super::super::{
    kaspa::profile_portal,
    support::util::{open_wallet_secret, required, required_str, to_value},
};

const HANDSHAKE_AMOUNT_SOMPI: u64 = 20_000_000;
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

pub(in crate::native::browser_host) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "kasia_identity" => identity(args),
        "kasia_send_handshake" => send_handshake(args).await,
        "kasia_send_message" => send_message(args).await,
        _ => invoke_history(command, args).await,
    }
}


async fn invoke_history(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "kasia_history" => history(args).await,
        "kasia_received_handshakes" => received_handshakes(args).await,
        _ => Err(format!("unknown browser Kasia command: {command}")),
    }
}

fn identity(args: &Value) -> Result<Value, String> {
    let password = required_str(args, "password")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let public: WalletPublic = required(args, "public")?;
    let secret = open_wallet_secret(password, &sealed, &public)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let public_key = public_key_from_secret(&identity_secret).map_err(|error| error.to_string())?;
    identity_secret.zeroize();
    to_value(KasiaIdentityProjection { public_key_hex: hex::encode(public_key) })
}

async fn send_handshake(args: &Value) -> Result<Value, String> {
    let request: KasiaSendRequest<WalletPublic> = required(args, "request")?;
    let secret = open_wallet_secret(&request.password, &request.sealed, &request.public)?;
    let peer_key = ghost_kaspa::p2pk_compressed_public_key(&request.mapping.kaspa_address)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let public_key = public_key_from_secret(&identity_secret).map_err(|error| error.to_string())?;
    identity_secret.zeroize();
    let handshake = KasiaHandshake::request(
        Some(request.mapping.our_alias.clone()),
        hex::encode(public_key),
        js_sys::Date::now().max(0.0) as u64,
        request.mapping.conversation_id.clone(),
        request.mapping.kaspa_address.clone(),
    );
    let handshake = if request.is_response { handshake.response() } else { handshake };
    let encrypted = encrypt_for(&peer_key, &handshake.encode()?).map_err(|error| error.to_string())?;
    let payload = encode_handshake(&encrypted);
    send_payload(request, secret, payload.as_bytes(), Some(HANDSHAKE_AMOUNT_SOMPI)).await
}

async fn send_message(args: &Value) -> Result<Value, String> {
    let request: KasiaSendRequest<WalletPublic> = required(args, "request")?;
    if request.text.trim().is_empty() {
        return Err("Kasia message must not be empty".into());
    }
    if !request.mapping.established {
        return Err("Kasia conversation handshake is not established".into());
    }
    let secret = open_wallet_secret(&request.password, &request.sealed, &request.public)?;
    let peer_key = ghost_kaspa::p2pk_compressed_public_key(&request.mapping.kaspa_address)?;
    let encrypted = encrypt_for(&peer_key, request.text.as_bytes()).map_err(|error| error.to_string())?;
    let payload = encode_comm(&request.mapping.our_alias, &encrypted)?;
    send_payload(request, secret, payload.as_bytes(), None).await
}

async fn send_payload(
    request: KasiaSendRequest<WalletPublic>,
    secret: WalletSecret,
    payload: &[u8],
    amount: Option<u64>,
) -> Result<Value, String> {
    let portal = profile_portal(
        &request.profile_id,
        &request.public,
        request.wrpc_endpoint.as_deref(),
    ).await?;
    let sent = if let Some(amount) = amount {
        wallet::send_payload_amount(
            &portal, &secret, &request.public, &request.mapping.kaspa_address,
            amount, request.fee_sompi, payload,
        ).await?
    } else {
        let stable = request.public.receive_addresses.first()
            .ok_or("wallet has no Kasia self-stash address")?;
        wallet::send_payload(
            &portal, &secret, &request.public, stable, request.fee_sompi, payload,
        ).await?
    };
    to_value(BroadcastResult {
        transaction_id: sent.transaction_id,
        fee_sompi: sent.fee_sompi,
        public: sent.public.projection(),
    })
}

async fn history(args: &Value) -> Result<Value, String> {
    let request: KasiaHistoryRequest<WalletPublic> = required(args, "request")?;
    let their_alias = request.mapping.their_alias.as_deref()
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
    ).await?;
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

async fn received_handshakes(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let password = required_str(args, "password")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let public: WalletPublic = required(args, "public")?;
    let indexer_url = required_str(args, "indexerUrl")?;
    let block_time = args.get("blockTime").and_then(Value::as_u64).unwrap_or(0);
    let own = public.receive_addresses.first().ok_or("wallet has no Kasia receiving address")?;
    let secret = open_wallet_secret(password, &sealed, &public)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let values: Vec<HandshakeResponse> = paginated(
        indexer_url, "/handshakes/by-receiver", &[("address", own.as_str())], block_time,
    ).await?;
    let handshakes = values.into_iter()
        .filter_map(|value| decrypt_handshake(value, &identity_secret).ok())
        .collect::<Vec<_>>();
    identity_secret.zeroize();
    super::super::support::debug::record("debug", "kasia", "received-handshakes", format!("profile={profile_id} count={}", handshakes.len()));
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
    let text = String::from_utf8(plaintext).map_err(|_| "Kasia message is not UTF-8".to_string())?;
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

async fn paginated<T: for<'de> Deserialize<'de>>(
    base: &str,
    endpoint: &str,
    params: &[(&str, &str)],
    start: u64,
) -> Result<Vec<T>, String>
where
    T: BlockTimed,
{
    validate_indexer(base)?;
    let mut cursor = start;
    let mut all = Vec::new();
    for _ in 0..MAX_PAGES {
        let mut url = format!("{}{}?limit={PAGE_LIMIT}&block_time={cursor}", base.trim_end_matches('/'), endpoint);
        for (key, value) in params {
            url.push('&');
            url.push_str(key);
            url.push('=');
            url.push_str(&encode(value));
        }
        let page: Vec<T> = fetch_json(&url).await?;
        let count = page.len();
        let next = page.iter().filter_map(BlockTimed::block_time).max().unwrap_or(cursor);
        all.extend(page);
        if count < PAGE_LIMIT || next <= cursor { break; }
        cursor = next;
    }
    Ok(all)
}

trait BlockTimed { fn block_time(&self) -> Option<u64>; }
impl BlockTimed for HandshakeResponse { fn block_time(&self) -> Option<u64> { self.block_time } }
impl BlockTimed for ContextResponse { fn block_time(&self) -> Option<u64> { self.block_time } }

async fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let window = web_sys::window().ok_or("Browser window unavailable")?;
    let response = JsFuture::from(window.fetch_with_str(url)).await
        .map_err(crate::native::invoke::js_error)?
        .dyn_into::<Response>().map_err(|_| "Kasia indexer returned a non-HTTP response".to_string())?;
    if !response.ok() { return Err(format!("Kasia indexer returned HTTP {}", response.status())); }
    let text = JsFuture::from(response.text().map_err(crate::native::invoke::js_error)?).await
        .map_err(crate::native::invoke::js_error)?.as_string().ok_or("Kasia indexer response was not text")?;
    serde_json::from_str(&text).map_err(|error| format!("invalid Kasia indexer response: {error}"))
}

fn validate_indexer(base: &str) -> Result<(), String> {
    let base = base.trim();
    if base.starts_with("https://") || base.starts_with("http://localhost") || base.starts_with("http://127.0.0.1") {
        Ok(())
    } else {
        Err("Kasia indexer must use HTTPS (localhost is allowed for development)".into())
    }
}

fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value).as_string().unwrap_or_default()
}
