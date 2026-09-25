use super::PublicDirectoryState;
use ghost_api::PublishedGhostDescriptor;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use tauri::State;

struct PublishInput {
    profile_id: String,
    password: String,
    identity_id: String,
    display_name: String,
    username: String,
    description: String,
    interests: Vec<String>,
    avatar: Option<ghost_media::MediaReference>,
    discoverable: bool,
    sealed: Vec<u8>,
    public: WalletPublic,
    wrpc_endpoint: Option<String>,
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn publish_ghost_descriptor(
    state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    directory: State<'_, PublicDirectoryState>,
    profile_id: String,
    password: String,
    identity_id: String,
    display_name: String,
    username: String,
    description: String,
    interests: Vec<String>,
    avatar: Option<ghost_media::MediaReference>,
    discoverable: bool,
    sealed: Vec<u8>,
    public: WalletPublic,
    rest_endpoint: Option<String>,
    wrpc_endpoint: Option<String>,
) -> Result<PublishedGhostDescriptor, String> {
    let _ = rest_endpoint;
    let input = PublishInput {
        profile_id,
        password,
        identity_id,
        display_name,
        username,
        description,
        interests,
        avatar,
        discoverable,
        sealed,
        public,
        wrpc_endpoint,
    };
    let outbound_lock = wallet_state.outbound_lock(&input.profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    publish_descriptor(state.inner(), gateway.inner(), &directory, input).await
}

async fn publish_descriptor(
    state: &crate::hydra_commands::HydraRuntimeState,
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    directory: &PublicDirectoryState,
    input: PublishInput,
) -> Result<PublishedGhostDescriptor, String> {
    let secret = crate::wallet_commands::open_secret(&input.password, &input.sealed)?;
    crate::wallet_commands::validate_public_projection(&secret, &input.public)?;
    let stable_address = crate::wallet_commands::stable_address(&input.public)?;
    let card = contact_card(state, &input.profile_id, &input.identity_id).await?;
    let descriptor = super::build_signed_descriptor(
        &secret,
        &input.public,
        &card,
        &input.identity_id,
        &input.display_name,
        input.discoverable,
        &input.username,
        &input.description,
        &input.interests,
        input.avatar.clone(),
    )?;
    let payload = descriptor.encode()?;
    if directory_matches(directory, &stable_address, &payload)? {
        return Ok(descriptor_result(
            stable_address,
            None,
            input.discoverable,
            &input.public,
        ));
    }
    let result = publish_payload(
        gateway,
        &secret,
        &input.public,
        &stable_address,
        &payload,
        input.wrpc_endpoint.as_deref(),
    )
    .await?;
    Ok(descriptor_result(
        stable_address,
        Some(result.transaction_id),
        input.discoverable,
        &result.public,
    ))
}

async fn contact_card(
    state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    identity_id: &str,
) -> Result<Vec<u8>, String> {
    let runtime = state.runtime(profile_id)?;
    let runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    runtime.hydra.contact_card()
}

fn directory_matches(
    directory: &PublicDirectoryState,
    address: &str,
    payload: &[u8],
) -> Result<bool, String> {
    let Some((current, _)) = directory.latest(address) else {
        return Ok(false);
    };
    Ok(current.encode()? == payload)
}

async fn publish_payload(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    secret: &WalletSecret,
    public: &WalletPublic,
    stable_address: &str,
    payload: &[u8],
    wrpc_endpoint: Option<&str>,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    let portal = gateway.portal(public, wrpc_endpoint).await?;
    match ghost_kaspa::wallet::send_payload(&portal, secret, public, stable_address, 0, payload)
        .await
    {
        Ok(result) => Ok(result),
        Err(error) => {
            gateway.note_operation_error(&error).await;
            Err(error)
        }
    }
}

fn descriptor_result(
    kaspa_address: String,
    transaction_id: Option<String>,
    discoverable: bool,
    public: &WalletPublic,
) -> PublishedGhostDescriptor {
    PublishedGhostDescriptor {
        kaspa_address,
        already_current: transaction_id.is_none(),
        transaction_id,
        discoverable,
        public: crate::wallet_commands::wallet_projection(public),
    }
}
