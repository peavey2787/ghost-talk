use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{DirectoryLiveEvent, PublicGhostProfile, ResolvedGhostPeer};
use ghost_indexer::GhostProfileIndex;
use ghost_kaspa::LiveTransactionObservation;
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};

use super::super::{
    support::util::{required_str, to_value},
    HYDRA_RUNTIMES,
};

thread_local! {
    static DIRECTORIES: RefCell<HashMap<String, GhostProfileIndex>> = RefCell::new(HashMap::new());
}

pub(in crate::native::browser_host) fn invoke(
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    match command {
        "resolve_ghost_peer" => resolve_peer(args),
        "lookup_ghost_profile" => lookup_profile(args),
        _ => Err(format!("unknown browser peer command: {command}")),
    }
}

pub(in crate::native::browser_host) fn ingest_live(
    profile_id: &str,
    network: &str,
    observations: &[LiveTransactionObservation],
    daa_score: u64,
) {
    let index = directory(network);
    for observation in observations {
        index.ingest_payload(&observation.payload, observation.daa_score, daa_score);
    }
    let public_profiles = latest_public_profiles(&index, observations);
    if public_profiles.is_empty() {
        return;
    }
    let event = DirectoryLiveEvent {
        profile_id: profile_id.to_owned(),
        directory_checkpoint: daa_score.to_string(),
        public_profiles,
    };
    if let Ok(payload) = serde_json::to_value(event) {
        crate::native::events::emit_browser("ghost://directory-live", payload);
    }
}

fn resolve_peer(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let identity_id = required_str(args, "identityId")?;
    let target = required_str(args, "target")?;
    let network = required_str(args, "network")?;
    let (address, descriptor, blue_score) = resolve_descriptor(network, target)?;
    let Some(descriptor) = descriptor else {
        return to_value(unresolved(address, blue_score));
    };
    if !descriptor.discoverable {
        return to_value(unresolved(address, blue_score));
    }
    let contact_handle = preview_contact(profile_id, identity_id, &descriptor)?;
    to_value(ResolvedGhostPeer {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_handle: Some(contact_handle),
        descriptor_blue_score: blue_score,
        kns_name: None,
        dotk_name: None,
        verified_public: true,
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
        avatar: descriptor.avatar,
        capabilities: descriptor.capabilities,
    })
}

fn lookup_profile(args: &Value) -> Result<Value, String> {
    let target = required_str(args, "target")?;
    let network = required_str(args, "network")?;
    let (_, descriptor, blue_score) = resolve_descriptor(network, target)?;
    let Some(descriptor) = descriptor.filter(|item| item.discoverable) else {
        return Ok(Value::Null);
    };
    to_value(Some(PublicGhostProfile {
        kaspa_address: descriptor.kaspa_address,
        display_name: descriptor.display_name,
        hydra_identity_id: descriptor.hydra_identity_id,
        descriptor_blue_score: blue_score.unwrap_or_default(),
        username: descriptor.username,
        description: descriptor.description,
        interests: descriptor.interests,
        avatar: descriptor.avatar,
        capabilities: descriptor.capabilities,
        signature: descriptor.signature_hex,
        verified: true,
        discoverable: true,
        ..Default::default()
    }))
}

fn resolve_descriptor(
    network: &str,
    target: &str,
) -> Result<
    (
        String,
        Option<ghost_protocol::GhostContactDescriptor>,
        Option<String>,
    ),
    String,
> {
    let index = directory(network);
    if ghost_kaspa::validate_destination(target).is_ok() {
        let latest = index.latest(target);
        return Ok((
            target.to_owned(),
            latest.as_ref().map(|(descriptor, _)| descriptor.clone()),
            latest.map(|(_, score)| score.to_string()),
        ));
    }
    let needle = target.trim().trim_start_matches('@');
    let matches = index
        .all()
        .into_iter()
        .filter(|(descriptor, _)| descriptor.username.eq_ignore_ascii_case(needle))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [(descriptor, score)] => Ok((
            descriptor.kaspa_address.clone(),
            Some(descriptor.clone()),
            Some(score.to_string()),
        )),
        [] => Err(format!("No live Ghost profile matches {target}")),
        _ => Err(format!(
            "Multiple live Ghost profiles match {target}; use a Kaspa address"
        )),
    }
}

fn preview_contact(
    profile_id: &str,
    identity_id: &str,
    descriptor: &ghost_protocol::GhostContactDescriptor,
) -> Result<String, String> {
    let card = BASE64
        .decode(&descriptor.hydra_contact_card_b64)
        .map_err(|_| "GTCD HYDRA contact card is not valid base64".to_string())?;
    HYDRA_RUNTIMES.with(|runtimes| {
        let runtimes = runtimes.borrow();
        let hydra = runtimes
            .get(profile_id)
            .ok_or_else(|| "HYDRA profile must be unlocked before resolving peers".to_string())?;
        let selected = hydra
            .list_identities()
            .into_iter()
            .find(|identity| identity.id == identity_id && identity.unlocked)
            .ok_or_else(|| "selected HYDRA identity is not unlocked".to_string())?;
        let contact = hydra.preview_contact(&card)?;
        if descriptor.hydra_identity_id != contact.handle {
            return Err("GTCD HYDRA identity does not match its authenticated contact card".into());
        }
        if selected.id == contact.handle {
            return Err("cannot resolve this Ghost Talk identity as its own peer".into());
        }
        Ok(contact.handle)
    })
}

fn unresolved(address: String, blue_score: Option<String>) -> ResolvedGhostPeer {
    ResolvedGhostPeer {
        kaspa_address: address,
        descriptor_blue_score: blue_score,
        ..Default::default()
    }
}

fn directory(network: &str) -> GhostProfileIndex {
    DIRECTORIES.with(|directories| {
        directories
            .borrow_mut()
            .entry(network.to_owned())
            .or_default()
            .clone()
    })
}

fn latest_public_profiles(
    index: &GhostProfileIndex,
    observations: &[LiveTransactionObservation],
) -> Vec<PublicGhostProfile> {
    let mut profiles = HashMap::<String, PublicGhostProfile>::new();
    for observation in observations {
        if !observation.payload.starts_with(&ghost_protocol::GTCD_MAGIC) {
            continue;
        }
        let Ok(candidate) = ghost_protocol::GhostContactDescriptor::decode(&observation.payload)
        else {
            continue;
        };
        let Some((descriptor, score)) = index.latest(&candidate.kaspa_address) else {
            continue;
        };
        if score != observation.daa_score || !descriptor.discoverable {
            continue;
        }
        profiles.insert(
            descriptor.kaspa_address.clone(),
            PublicGhostProfile {
                kaspa_address: descriptor.kaspa_address,
                display_name: descriptor.display_name,
                hydra_identity_id: descriptor.hydra_identity_id,
                descriptor_blue_score: score.to_string(),
                username: descriptor.username,
                description: descriptor.description,
                interests: descriptor.interests,
                avatar: descriptor.avatar,
                capabilities: descriptor.capabilities,
                signature: descriptor.signature_hex,
                verified: true,
                discoverable: true,
                ..Default::default()
            },
        );
    }
    profiles.into_values().collect()
}
