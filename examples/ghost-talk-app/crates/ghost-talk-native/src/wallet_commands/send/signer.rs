use super::support::parse_u64_decimal;
use crate::wallet_commands::wallet_lifecycle::{
    broadcast_projection, BroadcastResult, State, WalletNetworkOptions, WalletPublic,
    WalletRuntimeState,
};

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn wallet_prepare_signer_send(
    state: State<'_, WalletRuntimeState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    profile_id: String,
    public: WalletPublic,
    destination: String,
    amount_sompi: String,
    fee_sompi: String,
    options: WalletNetworkOptions,
) -> Result<String, String> {
    let outbound_lock = state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    let amount = parse_u64_decimal(&amount_sompi, "amount")?;
    let fee = parse_u64_decimal(&fee_sompi, "fee")?;
    let portal = crate::mailbox_commands::outbound_portal(
        &gateway,
        &public,
        options.wrpc_endpoint.as_deref(),
    )
    .await?;
    ghost_kaspa::wallet::prepare_signer_send(&portal, &public, &destination, amount, fee).await
}

#[tauri::command]
pub async fn wallet_broadcast_signer_send(
    state: State<'_, WalletRuntimeState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    profile_id: String,
    public: WalletPublic,
    signed_pskt: String,
    options: WalletNetworkOptions,
) -> Result<BroadcastResult, String> {
    let outbound_lock = state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    let portal = crate::mailbox_commands::outbound_portal(
        &gateway,
        &public,
        options.wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = ghost_kaspa::wallet::broadcast_signer_send(&portal, &public, &signed_pskt).await;
    if let Err(error) = &result {
        gateway.note_operation_error(error).await;
    }
    result.map(broadcast_projection)
}
