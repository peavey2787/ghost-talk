use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
pub(crate) use ghost_api::PublicGhostProfile;
use ghost_api::ResolvedGhostPeer;
use ghost_protocol::GhostContactDescriptor;
use tauri::State;

mod name_resolution;
pub(crate) use name_resolution::NameResolverState;
use name_resolution::{resolve_target, ResolvedTarget};

const MAX_CONTACT_CARD_BYTES: usize = 64 * 1024;

/// Current Ghost public-directory projection learned from verified Kaspa L1 payloads.
pub type PublicDirectoryState = ghost_indexer::GhostProfileIndex;

fn unresolved_peer(
    target: ResolvedTarget,
    descriptor_blue_score: Option<String>,
) -> ResolvedGhostPeer {
    let kns_name = target.kns_name();
    let dotk_name = target.dotk_name();
    ResolvedGhostPeer {
        kaspa_address: target.address,
        display_name: String::new(),
        hydra_handle: None,
        descriptor_blue_score,
        kns_name,
        dotk_name,
        verified_public: false,
        username: String::new(),
        description: String::new(),
        interests: Vec::new(),
        avatar: None,
        capabilities: Vec::new(),
    }
}

#[tauri::command]
pub async fn lookup_ghost_profile(
    names: State<'_, NameResolverState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    directory: State<'_, PublicDirectoryState>,
    target: String,
    network: String,
    rest_endpoint: Option<String>,
    wrpc_endpoint: Option<String>,
) -> Result<Option<PublicGhostProfile>, String> {
    let _ = rest_endpoint;
    let target = resolve_target(
        &names,
        &gateway,
        &target,
        &network,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let Some((descriptor, blue_score)) = directory.latest(&target.address) else {
        // Current discovery comes only from the live Kaspa BlockAdded stream.
        // Do not query REST and misrepresent an archival descriptor as current.
        return Ok(None);
    };
    if !descriptor.discoverable {
        return Ok(None);
    }
    Ok(Some(project_public_profile(
        target,
        descriptor,
        blue_score.to_string(),
    )?))
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn resolve_ghost_peer(
    names: State<'_, NameResolverState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    directory: State<'_, PublicDirectoryState>,
    profile_id: String,
    _password: String,
    identity_id: String,
    target: String,
    network: String,
    rest_endpoint: Option<String>,
    wrpc_endpoint: Option<String>,
) -> Result<ResolvedGhostPeer, String> {
    let target = resolve_target(
        &names,
        &gateway,
        &target,
        &network,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    // Public discovery is an optimization, never a prerequisite for a private
    // address-only bootstrap. Current profile state is learned from the live
    // Kaspa BlockAdded stream only; REST is archival history and is never used
    // to decide who is currently discoverable.
    let _ = rest_endpoint;
    let Some((descriptor, blue_score)) = directory.latest(&target.address) else {
        return Ok(unresolved_peer(target, None));
    };
    if !descriptor.discoverable {
        return Ok(unresolved_peer(target, Some(blue_score.to_string())));
    }

    let contact_handle =
        preview_verified_contact(&state, &profile_id, &identity_id, &descriptor).await?;
    Ok(project_resolved_peer(
        target,
        descriptor,
        blue_score.to_string(),
        contact_handle,
    ))
}

fn project_public_profile(
    target: ResolvedTarget,
    descriptor: GhostContactDescriptor,
    descriptor_blue_score: String,
) -> Result<PublicGhostProfile, String> {
    if descriptor.hydra_identity_id.len() != 64
        || !descriptor
            .hydra_identity_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("latest public Ghost Talk profile has an invalid HYDRA identity id".into());
    }
    Ok(PublicGhostProfile {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_identity_id: descriptor.hydra_identity_id,
        descriptor_blue_score,
        primary_name: target.primary_name.clone(),
        verified_names: target.verified_names.clone(),
        kns_name: target.kns_name(),
        dotk_name: target.dotk_name(),
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
        avatar: descriptor.avatar,
        capabilities: descriptor.capabilities,
        signature: descriptor.signature_hex,
        verified: true,
        discoverable: true,
    })
}

fn project_resolved_peer(
    target: ResolvedTarget,
    descriptor: GhostContactDescriptor,
    descriptor_blue_score: String,
    contact_handle: String,
) -> ResolvedGhostPeer {
    ResolvedGhostPeer {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_handle: Some(contact_handle),
        descriptor_blue_score: Some(descriptor_blue_score),
        kns_name: target.kns_name(),
        dotk_name: target.dotk_name(),
        verified_public: true,
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
        avatar: descriptor.avatar,
        capabilities: descriptor.capabilities,
    }
}

async fn preview_verified_contact(
    state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    identity_id: &str,
    descriptor: &GhostContactDescriptor,
) -> Result<String, String> {
    let card = decode_public_contact_card(descriptor)?;
    let runtime = state.runtime(profile_id)?;
    let runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    // Public discovery verifies a card but does not grant contact consent.
    let contact = runtime.hydra.preview_contact(&card)?;
    if !descriptor.hydra_identity_id.is_empty() && descriptor.hydra_identity_id != contact.handle {
        return Err("GTCD HYDRA identity id does not match its authenticated contact card".into());
    }
    Ok(contact.handle)
}

fn decode_public_contact_card(descriptor: &GhostContactDescriptor) -> Result<Vec<u8>, String> {
    let card = BASE64
        .decode(&descriptor.hydra_contact_card_b64)
        .map_err(|_| "GTCD HYDRA contact card is not valid base64".to_string())?;
    if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
        Err("GTCD HYDRA contact card size is invalid".into())
    } else {
        Ok(card)
    }
}

mod descriptor;
pub(super) use descriptor::{build_private_descriptor, build_signed_descriptor};

mod publish;
pub use publish::publish_ghost_descriptor;
