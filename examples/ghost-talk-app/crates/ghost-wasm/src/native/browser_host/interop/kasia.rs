use ghost_api::BroadcastResult;
use ghost_kasia::{
    encode_comm, encode_handshake, encrypt_for, public_key_from_secret, KasiaHandshake,
    KasiaIdentityProjection, KasiaSendRequest,
};
use ghost_kaspa::wallet::{self, WalletPublic, WalletSecret};
use serde_json::Value;
use zeroize::Zeroize;

use super::super::{
    kaspa::profile_portal,
    support::util::{open_wallet_secret, required, required_str, to_value},
};

mod indexer;

const HANDSHAKE_AMOUNT_SOMPI: u64 = 20_000_000;
pub(in crate::native::browser_host) async fn invoke(
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    match command {
        "kasia_identity" => identity(args),
        "kasia_send_handshake" => send_handshake(args).await,
        "kasia_send_message" => send_message(args).await,
        _ => invoke_history(command, args).await,
    }
}

async fn invoke_history(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "kasia_history" => indexer::history(args).await,
        "kasia_received_handshakes" => indexer::received_handshakes(args).await,
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
    to_value(KasiaIdentityProjection {
        public_key_hex: hex::encode(public_key),
    })
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
    let handshake = if request.is_response {
        handshake.response()
    } else {
        handshake
    };
    let encrypted =
        encrypt_for(&peer_key, &handshake.encode()?).map_err(|error| error.to_string())?;
    let payload = encode_handshake(&encrypted);
    send_payload(
        request,
        secret,
        payload.as_bytes(),
        Some(HANDSHAKE_AMOUNT_SOMPI),
    )
    .await
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
    let encrypted =
        encrypt_for(&peer_key, request.text.as_bytes()).map_err(|error| error.to_string())?;
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
    )
    .await?;
    let sent = if let Some(amount) = amount {
        wallet::send_payload_amount(
            &portal,
            &secret,
            &request.public,
            &request.mapping.kaspa_address,
            amount,
            request.fee_sompi,
            payload,
        )
        .await?
    } else {
        let stable = request
            .public
            .receive_addresses
            .first()
            .ok_or("wallet has no Kasia self-stash address")?;
        wallet::send_payload(
            &portal,
            &secret,
            &request.public,
            stable,
            request.fee_sompi,
            payload,
        )
        .await?
    };
    to_value(BroadcastResult {
        transaction_id: sent.transaction_id,
        fee_sompi: sent.fee_sompi,
        public: sent.public.projection(),
    })
}
