use crate::model::{PublishedGhostDescriptor, WalletProjection};
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use ghost_media::MediaReference;
use serde_json::Value;

use super::{
    kaspa::{endpoint_override, profile_portal},
    support::util::{required, required_str, to_value},
    HYDRA_RUNTIMES,
};

struct PublishInput {
    profile_id: String,
    password: String,
    identity_id: String,
    display_name: String,
    username: String,
    description: String,
    interests: Vec<String>,
    avatar: Option<MediaReference>,
    discoverable: bool,
    sealed: Vec<u8>,
    public: WalletProjection,
    wrpc_endpoint: Option<String>,
}

pub(super) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "publish_ghost_descriptor" => publish(args).await,
        _ => Err(format!("unknown browser Discover command: {command}")),
    }
}

async fn publish(args: &Value) -> Result<Value, String> {
    let input = parse_input(args)?;
    let public = WalletPublic::from_projection(&input.public);
    let secret: WalletSecret =
        ghost_storage::open_json(&input.password, &input.sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    let stable_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let contact_card = contact_card(&input.profile_id, &input.identity_id)?;
    let descriptor = ghost_kaspa::build_signed_descriptor(
        &secret,
        &public,
        &contact_card,
        &input.identity_id,
        &input.display_name,
        input.discoverable,
        &input.username,
        &input.description,
        &input.interests,
        input.avatar,
    )?;
    let payload = descriptor.encode()?;
    let portal = profile_portal(&input.profile_id, &public, input.wrpc_endpoint.as_deref()).await?;
    let result =
        ghost_kaspa::wallet::send_payload(&portal, &secret, &public, &stable_address, 0, &payload)
            .await?;
    to_value(PublishedGhostDescriptor {
        kaspa_address: stable_address,
        transaction_id: Some(result.transaction_id),
        already_current: false,
        discoverable: input.discoverable,
        public: result.public.projection(),
    })
}

fn parse_input(args: &Value) -> Result<PublishInput, String> {
    Ok(PublishInput {
        profile_id: required_str(args, "profileId")?.to_owned(),
        password: required_str(args, "password")?.to_owned(),
        identity_id: required_str(args, "identityId")?.to_owned(),
        display_name: required_str(args, "displayName")?.to_owned(),
        username: required_str(args, "username")?.to_owned(),
        description: required_str(args, "description")?.to_owned(),
        interests: required(args, "interests")?,
        avatar: required(args, "avatar")?,
        discoverable: required(args, "discoverable")?,
        sealed: required(args, "sealed")?,
        public: required(args, "public")?,
        wrpc_endpoint: endpoint_override(args, "wrpcEndpoint").map(str::to_owned),
    })
}

fn contact_card(profile_id: &str, identity_id: &str) -> Result<Vec<u8>, String> {
    HYDRA_RUNTIMES.with(|runtimes| {
        let runtimes = runtimes.borrow();
        let hydra = runtimes
            .get(profile_id)
            .ok_or_else(|| "HYDRA profile must be unlocked before publishing".to_string())?;
        let matches = hydra
            .list_identities()
            .into_iter()
            .any(|identity| identity.id == identity_id && identity.unlocked);
        if !matches {
            return Err("selected HYDRA identity is not unlocked in this browser profile".into());
        }
        hydra.contact_card()
    })
}
