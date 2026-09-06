use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_kaspa::wallet::WalletPublic;
use ghost_protocol::GhostContactDescriptor;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::State;

const GTCD_VERSION: u16 = 1;
const MAX_CONTACT_CARD_BYTES: usize = 64 * 1024;
const MAX_DISPLAY_NAME_CHARS: usize = 96;
const MAX_USERNAME_CHARS: usize = 48;
const MAX_DESCRIPTION_CHARS: usize = 320;
const MAX_INTERESTS: usize = 12;
const MAX_INTEREST_CHARS: usize = 32;

#[derive(Clone)]
struct CachedDescriptor {
    descriptor: GhostContactDescriptor,
    blue_score: u64,
}

/// Current Ghost public-directory state learned only from the live Kaspa
/// BlockAdded stream on the single Portal connection. REST/indexer history is
/// deliberately excluded from this current-state cache.
#[derive(Clone, Default)]
pub struct PublicDirectoryState {
    inner: Arc<Mutex<HashMap<String, CachedDescriptor>>>,
}

impl PublicDirectoryState {
    pub fn ingest_payload(&self, payload: &[u8], blue_score: u64, current_daa: u64) {
        if !payload.starts_with(&ghost_protocol::GTCD_MAGIC) {
            return;
        }
        let Ok(descriptor) = GhostContactDescriptor::decode(payload) else {
            return;
        };
        if descriptor.version != GTCD_VERSION
            || descriptor.expires_daa.is_some_and(|expires| current_daa != 0 && expires <= current_daa)
            || ghost_kaspa::validate_destination(&descriptor.kaspa_address).is_err()
            || ghost_kaspa::verify_gtcd(&descriptor).is_err()
        {
            return;
        }
        let Ok(mut entries) = self.inner.lock() else {
            return;
        };
        if entries
            .get(&descriptor.kaspa_address)
            .is_some_and(|existing| existing.blue_score > blue_score)
        {
            return;
        }
        entries.insert(
            descriptor.kaspa_address.clone(),
            CachedDescriptor { descriptor, blue_score },
        );
    }

    fn latest(&self, address: &str) -> Option<(GhostContactDescriptor, u64)> {
        self.inner
            .lock()
            .ok()
            .and_then(|entries| entries.get(address).cloned())
            .map(|entry| (entry.descriptor, entry.blue_score))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedGhostPeer {
    pub kaspa_address: String,
    pub display_name: String,
    pub hydra_handle: Option<String>,
    pub descriptor_blue_score: Option<String>,
    pub kns_name: Option<String>,
    pub verified_public: bool,
    pub username: String,
    pub description: String,
    pub interests: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicGhostProfile {
    pub kaspa_address: String,
    pub display_name: String,
    pub hydra_identity_id: String,
    pub descriptor_blue_score: String,
    pub kns_name: Option<String>,
    pub username: String,
    pub description: String,
    pub interests: Vec<String>,
    pub verified: bool,
    pub discoverable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublishedGhostDescriptor {
    pub kaspa_address: String,
    pub transaction_id: Option<String>,
    pub already_current: bool,
    pub discoverable: bool,
    pub public: WalletPublic,
}

struct ResolvedTarget {
    address: String,
    kns_name: Option<String>,
}

async fn resolve_target(target: &str, network: &str) -> Result<ResolvedTarget, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("Enter a Kaspa address or KNS name".into());
    }
    let kns_name = if target.to_ascii_lowercase().ends_with(".kas") {
        if network.trim().to_ascii_lowercase() != "mainnet" {
            return Err("KNS names resolve on Kaspa mainnet; use a Kaspa address on testnet".into());
        }
        Some(ghost_names::KnsResolver::normalize(target)?)
    } else {
        None
    };
    let address = match &kns_name {
        Some(name) => ghost_names::KnsResolver::default().resolve_owner(name).await?,
        None => target.to_owned(),
    };
    let parsed = ghost_kaspa::validate_destination(&address)?;
    Ok(ResolvedTarget {
        address: parsed.as_str().to_owned(),
        kns_name,
    })
}

#[tauri::command]
pub async fn lookup_ghost_profile(
    directory: State<'_, PublicDirectoryState>,
    target: String,
    network: String,
    rest_endpoint: Option<String>,
) -> Result<Option<PublicGhostProfile>, String> {
    let _ = rest_endpoint;
    let target = resolve_target(&target, &network).await?;
    let Some((descriptor, blue_score)) = directory.latest(&target.address) else {
        // Current discovery comes only from the live Kaspa BlockAdded stream.
        // Do not query REST and misrepresent an archival descriptor as current.
        return Ok(None);
    };
    if !descriptor.discoverable {
        return Ok(None);
    }
    if descriptor.hydra_identity_id.len() != 64
        || !descriptor.hydra_identity_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("latest public Ghost Talk profile has an invalid HYDRA identity id".into());
    }
    Ok(Some(PublicGhostProfile {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_identity_id: descriptor.hydra_identity_id,
        descriptor_blue_score: blue_score.to_string(),
        kns_name: target.kns_name,
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
        verified: true,
        discoverable: true,
    }))
}

#[tauri::command]
pub async fn resolve_ghost_peer(
    state: State<'_, super::hydra_commands::HydraRuntimeState>,
    directory: State<'_, PublicDirectoryState>,
    profile_id: String,
    _password: String,
    identity_id: String,
    target: String,
    network: String,
    rest_endpoint: Option<String>,
) -> Result<ResolvedGhostPeer, String> {
    let target = resolve_target(&target, &network).await?;
    // Public discovery is an optimization, never a prerequisite for a private
    // address-only bootstrap. Current profile state is learned from the live
    // Kaspa BlockAdded stream only; REST is archival history and is never used
    // to decide who is currently discoverable.
    let _ = rest_endpoint;
    let Some((descriptor, blue_score)) = directory.latest(&target.address) else {
        return Ok(ResolvedGhostPeer {
            kaspa_address: target.address,
            display_name: String::new(),
            hydra_handle: None,
            descriptor_blue_score: None,
            kns_name: target.kns_name,
            verified_public: false,
            username: String::new(),
            description: String::new(),
            interests: Vec::new(),
        });
    };
    if !descriptor.discoverable {
        return Ok(ResolvedGhostPeer {
            kaspa_address: target.address,
            display_name: String::new(),
            hydra_handle: None,
            descriptor_blue_score: Some(blue_score.to_string()),
            kns_name: target.kns_name,
            verified_public: false,
            username: String::new(),
            description: String::new(),
            interests: Vec::new(),
        });
    }

    let card = BASE64
        .decode(&descriptor.hydra_contact_card_b64)
        .map_err(|_| "GTCD HYDRA contact card is not valid base64".to_string())?;
    if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
        return Err("GTCD HYDRA contact card size is invalid".into());
    }
    let runtime = state.runtime(&profile_id)?;
    let runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    // Public discovery is verification, not consent. Merely looking up a GTCD
    // must not mutate HYDRA's persistent contact roster; the peer is added only
    // after the private GTCR/GTCA acceptance path succeeds.
    let contact = runtime.hydra.preview_contact(&card)?;
    if !descriptor.hydra_identity_id.is_empty() && descriptor.hydra_identity_id != contact.handle {
        return Err("GTCD HYDRA identity id does not match its authenticated contact card".into());
    }
    Ok(ResolvedGhostPeer {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_handle: Some(contact.handle),
        descriptor_blue_score: Some(blue_score.to_string()),
        kns_name: target.kns_name,
        verified_public: true,
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
    })
}

pub(super) fn build_signed_descriptor(
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    contact_card: &[u8],
    hydra_identity_id: &str,
    display_name: &str,
    discoverable: bool,
    username: &str,
    description: &str,
    interests: &[String],
) -> Result<GhostContactDescriptor, String> {
    if contact_card.is_empty() || contact_card.len() > MAX_CONTACT_CARD_BYTES {
        return Err("HYDRA contact card size is invalid".into());
    }
    if hydra_identity_id.len() != 64
        || !hydra_identity_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("HYDRA identity id must be exactly 64 hexadecimal characters".into());
    }
    let stable_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let normalized_interests = interests
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .take(MAX_INTERESTS)
        .map(|value| value.chars().take(MAX_INTEREST_CHARS).collect::<String>())
        .collect::<Vec<_>>();
    let mut descriptor = GhostContactDescriptor {
        version: GTCD_VERSION,
        kaspa_address: stable_address,
        hydra_contact_card_b64: BASE64.encode(contact_card),
        display_name: display_name.trim().chars().take(MAX_DISPLAY_NAME_CHARS).collect(),
        hydra_identity_id: hydra_identity_id.to_ascii_lowercase(),
        discoverable,
        username: if discoverable {
            username.trim().chars().take(MAX_USERNAME_CHARS).collect()
        } else {
            String::new()
        },
        description: if discoverable {
            description.trim().chars().take(MAX_DESCRIPTION_CHARS).collect()
        } else {
            String::new()
        },
        interests: if discoverable { normalized_interests } else { Vec::new() },
        capabilities: vec![
            "hydra-pq".into(),
            "kktp-v2".into(),
            "kaspa-mailbox".into(),
            "private-bootstrap".into(),
        ],
        expires_daa: None,
        signature_hex: String::new(),
    };
    let mut private_key = ghost_kaspa::wallet::receive_private_key(secret, 0)?;
    let result = ghost_kaspa::sign_gtcd(&mut descriptor, &private_key);
    zeroize::Zeroize::zeroize(&mut private_key);
    result?;
    Ok(descriptor)
}

pub(super) fn build_private_descriptor(
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    contact_card: &[u8],
    hydra_identity_id: &str,
    display_name: &str,
) -> Result<GhostContactDescriptor, String> {
    build_signed_descriptor(
        secret,
        public,
        contact_card,
        hydra_identity_id,
        display_name,
        false,
        "",
        "",
        &[],
    )
}

#[tauri::command]
pub async fn publish_ghost_descriptor(
    state: State<'_, super::hydra_commands::HydraRuntimeState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    directory: State<'_, PublicDirectoryState>,
    profile_id: String,
    password: String,
    identity_id: String,
    display_name: String,
    username: String,
    description: String,
    interests: Vec<String>,
    discoverable: bool,
    sealed: Vec<u8>,
    public: WalletPublic,
    rest_endpoint: Option<String>,
    wrpc_endpoint: Option<String>,
) -> Result<PublishedGhostDescriptor, String> {
    let secret = super::wallet_commands::open_secret(&password, &sealed)?;
    super::wallet_commands::validate_public_projection(&secret, &public)?;
    let stable_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let runtime = state.runtime(&profile_id)?;
    let card = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        runtime.hydra.contact_card()?
    };
    let descriptor = build_signed_descriptor(
        &secret,
        &public,
        &card,
        &identity_id,
        &display_name,
        discoverable,
        &username,
        &description,
        &interests,
    )?;
    let payload = descriptor.encode()?;

    // Duplicate suppression may use only the current live-node directory
    // cache. REST history can be older than the current descriptor and must not
    // decide whether a fresh publication is necessary.
    let _ = rest_endpoint;
    if let Some((current, _)) = directory.latest(&stable_address) {
        if current.encode()? == payload {
            return Ok(PublishedGhostDescriptor {
                kaspa_address: stable_address,
                transaction_id: None,
                already_current: true,
                discoverable,
                public,
            });
        }
    }

    let portal = gateway.portal(&public, wrpc_endpoint.as_deref()).await?;
    let result = match ghost_kaspa::wallet::send_payload(
        &portal,
        &secret,
        &public,
        &stable_address,
        0,
        &payload,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            gateway.note_operation_error(&error).await;
            return Err(error);
        }
    };
    Ok(PublishedGhostDescriptor {
        kaspa_address: stable_address,
        transaction_id: Some(result.transaction_id),
        already_current: false,
        discoverable,
        public: result.public,
    })
}
