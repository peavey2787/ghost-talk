mod progress;
mod publish;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_api::{KaspaArchivePlan, KaspaArchivePublishResult};
use ghost_kaspa::{validate_archive_size, ArchivePlanRequest, ArchivePublishRequest};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn kaspa_archive_plan(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: ArchivePlanRequest,
) -> Result<KaspaArchivePlan, String> {
    validate_archive_size(request.data_size)?;
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let address = crate::wallet_commands::stable_address(&request.public)?;
    let portal = crate::mailbox_commands::outbound_portal(
        gateway.inner(),
        &request.public,
        request.wrpc_endpoint.as_deref(),
    )
    .await?;
    ghost_kaspa::archive_plan(
        &portal,
        &secret,
        &request.public,
        &address,
        request.data_size,
    )
    .await
}

#[tauri::command]
pub async fn kaspa_archive_publish(
    app: AppHandle,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: ArchivePublishRequest,
) -> Result<KaspaArchivePublishResult, String> {
    ensure_confirmed(&request)?;
    let bytes = decode_archive_body(&request.data_base64)?;
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let lock = wallet_state.outbound_lock(&request.profile_id)?;
    let _guard = lock.lock().await;
    let address = crate::wallet_commands::stable_address(&request.public)?;
    let portal = crate::mailbox_commands::outbound_portal(
        gateway.inner(),
        &request.public,
        request.wrpc_endpoint.as_deref(),
    )
    .await?;
    publish::publish_archive(&app, &portal, &secret, &request, &address, bytes).await
}

fn ensure_confirmed(request: &ArchivePublishRequest) -> Result<(), String> {
    request
        .confirmed
        .then_some(())
        .ok_or_else(|| "permanent Kaspa archive requires explicit confirmation".into())
}

fn decode_archive_body(encoded: &str) -> Result<Vec<u8>, String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| format!("archive base64 failed: {error}"))?;
    validate_archive_size(bytes.len() as u64)?;
    Ok(bytes)
}
