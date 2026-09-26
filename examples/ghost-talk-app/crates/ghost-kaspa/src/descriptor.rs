use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_media::MediaReference;
use ghost_protocol::{GhostContactDescriptor, GTCD_VERSION};

use crate::wallet::{receive_private_key, WalletPublic, WalletSecret};

const MAX_CONTACT_CARD_BYTES: usize = 64 * 1024;
const MAX_DISPLAY_NAME_CHARS: usize = 96;
const MAX_USERNAME_CHARS: usize = 48;
const MAX_DESCRIPTION_CHARS: usize = 320;
const MAX_INTERESTS: usize = 12;
const MAX_INTEREST_CHARS: usize = 32;

fn validate_inputs(contact_card: &[u8], hydra_identity_id: &str) -> Result<(), String> {
    if contact_card.is_empty() || contact_card.len() > MAX_CONTACT_CARD_BYTES {
        return Err("HYDRA contact card size is invalid".into());
    }
    if hydra_identity_id.len() != 64
        || !hydra_identity_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("HYDRA identity id must be exactly 64 hexadecimal characters".into());
    }
    Ok(())
}

fn normalized_interests(interests: &[String]) -> Vec<String> {
    interests
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .take(MAX_INTERESTS)
        .map(|value| value.chars().take(MAX_INTEREST_CHARS).collect::<String>())
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit signed descriptor fields"
)]
fn unsigned_descriptor(
    public: &WalletPublic,
    contact_card: &[u8],
    hydra_identity_id: &str,
    display_name: &str,
    discoverable: bool,
    username: &str,
    description: &str,
    interests: &[String],
    avatar: Option<MediaReference>,
) -> Result<GhostContactDescriptor, String> {
    validate_inputs(contact_card, hydra_identity_id)?;
    let stable_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let (username, description, interests) =
        visibility_fields(discoverable, username, description, interests);
    Ok(GhostContactDescriptor {
        version: GTCD_VERSION,
        kaspa_address: stable_address,
        hydra_contact_card_b64: BASE64.encode(contact_card),
        display_name: display_name
            .trim()
            .chars()
            .take(MAX_DISPLAY_NAME_CHARS)
            .collect(),
        hydra_identity_id: hydra_identity_id.to_ascii_lowercase(),
        discoverable,
        username,
        description,
        interests,
        avatar: discoverable.then_some(avatar).flatten(),
        capabilities: ghost_api::default_capabilities(),
        expires_daa: None,
        signature_hex: String::new(),
    })
}

fn visibility_fields(
    discoverable: bool,
    username: &str,
    description: &str,
    interests: &[String],
) -> (String, String, Vec<String>) {
    if !discoverable {
        return Default::default();
    }
    (
        username.trim().chars().take(MAX_USERNAME_CHARS).collect(),
        description
            .trim()
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect(),
        normalized_interests(interests),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit signed descriptor fields"
)]
pub fn build_signed_descriptor(
    secret: &WalletSecret,
    public: &WalletPublic,
    contact_card: &[u8],
    hydra_identity_id: &str,
    display_name: &str,
    discoverable: bool,
    username: &str,
    description: &str,
    interests: &[String],
    avatar: Option<MediaReference>,
) -> Result<GhostContactDescriptor, String> {
    let mut descriptor = unsigned_descriptor(
        public,
        contact_card,
        hydra_identity_id,
        display_name,
        discoverable,
        username,
        description,
        interests,
        avatar,
    )?;
    let mut private_key = receive_private_key(secret, 0)?;
    let result = crate::sign_gtcd(&mut descriptor, &private_key);
    zeroize::Zeroize::zeroize(&mut private_key);
    result?;
    Ok(descriptor)
}

pub fn build_private_descriptor(
    secret: &WalletSecret,
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
        None,
    )
}
