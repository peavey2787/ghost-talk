use ghost_api::BroadcastResult;
use ghost_kasia::{
    decrypt_indexed_handshake, encode_comm, encode_handshake, encrypt_for, public_key_from_secret,
    KasiaConversationHistory, KasiaHandshake, KasiaHistoryRequest, KasiaIdentityProjection,
    KasiaIndexerClient, KasiaMessage, KasiaReceivedHandshake, KasiaSendRequest,
};
use ghost_kaspa::wallet::{self, WalletPublic};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use zeroize::Zeroize;

const HANDSHAKE_AMOUNT_SOMPI: u64 = 20_000_000;

#[tauri::command]
pub async fn kasia_identity(
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<KasiaIdentityProjection, String> {
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let public_key = public_key_from_secret(&identity_secret).map_err(|error| error.to_string())?;
    identity_secret.zeroize();
    Ok(KasiaIdentityProjection {
        public_key_hex: hex::encode(public_key),
    })
}

#[tauri::command]
pub async fn kasia_send_handshake(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: KasiaSendRequest<WalletPublic>,
) -> Result<BroadcastResult, String> {
    let peer_key = ghost_kaspa::p2pk_compressed_public_key(&request.mapping.kaspa_address)?;
    let public_key_hex = local_kasia_public_key_hex(&wallet_state, &request)?;
    let handshake = handshake_for_request(&request, public_key_hex)?;
    let encrypted =
        encrypt_for(&peer_key, &handshake.encode()?).map_err(|error| error.to_string())?;
    let payload = encode_handshake(&encrypted);
    send_kasia_payload(
        gateway,
        wallet_state,
        request,
        payload.as_bytes(),
        Some(HANDSHAKE_AMOUNT_SOMPI),
    )
    .await
}

#[tauri::command]
pub async fn kasia_send_message(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: KasiaSendRequest<WalletPublic>,
) -> Result<BroadcastResult, String> {
    if request.text.trim().is_empty() {
        return Err("Kasia message must not be empty".into());
    }
    if !request.mapping.established {
        return Err("Kasia conversation handshake is not established".into());
    }
    let peer_key = ghost_kaspa::p2pk_compressed_public_key(&request.mapping.kaspa_address)?;
    let encrypted =
        encrypt_for(&peer_key, request.text.as_bytes()).map_err(|error| error.to_string())?;
    let payload = encode_comm(&request.mapping.our_alias, &encrypted)?;
    send_kasia_payload(gateway, wallet_state, request, payload.as_bytes(), None).await
}

#[tauri::command]
pub async fn kasia_history(
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: KasiaHistoryRequest<WalletPublic>,
) -> Result<Vec<KasiaMessage>, String> {
    let their_alias = request
        .mapping
        .their_alias
        .as_deref()
        .ok_or_else(|| "Kasia peer alias is not established".to_string())?;
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let indexed = KasiaIndexerClient::new(request.indexer_url)?
        .contextual_by_sender(
            &request.mapping.kaspa_address,
            their_alias,
            request.block_time,
        )
        .await?;
    let mut history = KasiaConversationHistory::default();
    for transaction in indexed {
        let _ = history.ingest(transaction, &identity_secret, false);
    }
    identity_secret.zeroize();
    Ok(history.messages().to_vec())
}

#[tauri::command]
pub async fn kasia_received_handshakes(
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    indexer_url: String,
    block_time: u64,
) -> Result<Vec<KasiaReceivedHandshake>, String> {
    let own_address = public
        .receive_addresses
        .first()
        .ok_or_else(|| "wallet has no Kasia receiving address".to_string())?;
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let responses = KasiaIndexerClient::new(indexer_url)?
        .handshakes_by_receiver(own_address, block_time)
        .await?;
    let handshakes = responses
        .into_iter()
        .filter_map(|response| decrypt_indexed_handshake(response, &identity_secret).ok())
        .collect();
    identity_secret.zeroize();
    Ok(handshakes)
}

fn handshake_for_request(
    request: &KasiaSendRequest<WalletPublic>,
    public_key_hex: String,
) -> Result<KasiaHandshake, String> {
    let handshake = KasiaHandshake::request(
        Some(request.mapping.our_alias.clone()),
        public_key_hex,
        now_ms()?,
        request.mapping.conversation_id.clone(),
        request.mapping.kaspa_address.clone(),
    );
    Ok(if request.is_response {
        handshake.response()
    } else {
        handshake
    })
}

fn local_kasia_public_key_hex(
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    request: &KasiaSendRequest<WalletPublic>,
) -> Result<String, String> {
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let mut identity_secret = wallet::receive_private_key(&secret, 0)?;
    let public_key = public_key_from_secret(&identity_secret).map_err(|error| error.to_string())?;
    identity_secret.zeroize();
    Ok(hex::encode(public_key))
}

async fn send_kasia_payload(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: KasiaSendRequest<WalletPublic>,
    payload: &[u8],
    amount_sompi: Option<u64>,
) -> Result<BroadcastResult, String> {
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let lock = wallet_state.outbound_lock(&request.profile_id)?;
    let _guard = lock.lock().await;
    let portal = kasia_outbound_portal(gateway.inner(), &request).await?;
    let result = if let Some(amount) = amount_sompi {
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
        let stable_address = request
            .public
            .receive_addresses
            .first()
            .ok_or_else(|| "wallet has no Kasia self-stash address".to_string())?;
        wallet::send_payload(
            &portal,
            &secret,
            &request.public,
            stable_address,
            request.fee_sompi,
            payload,
        )
        .await?
    };
    Ok(crate::wallet_commands::broadcast_projection(result))
}

async fn kasia_outbound_portal(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    request: &KasiaSendRequest<WalletPublic>,
) -> Result<ghost_kaspa::PortalFacade, String> {
    crate::mailbox_commands::outbound_portal(
        gateway,
        &request.public,
        request.wrpc_endpoint.as_deref(),
    )
    .await
}

fn now_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before Unix epoch".to_string())?;
    u64::try_from(duration.as_millis())
        .map_err(|_| "system clock exceeds Kasia timestamp range".to_string())
}
