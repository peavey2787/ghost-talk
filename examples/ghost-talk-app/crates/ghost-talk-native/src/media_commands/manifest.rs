use ghost_kaspa::wallet::{private_key_for_address, WalletPublic};
use ghost_kaspa::{sign_domain_message, verify_domain_message};
use ghost_media::GhostMediaManifest;
use serde::Deserialize;
use tauri::State;

const MEDIA_MANIFEST_DOMAIN: &[u8] = b"GhostMediaManifest/v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSignRequest {
    pub profile_id: String,
    pub password: String,
    pub sealed: Vec<u8>,
    pub public: WalletPublic,
    pub manifest: GhostMediaManifest,
}

#[tauri::command]
pub fn media_manifest_sign(
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: ManifestSignRequest,
) -> Result<GhostMediaManifest, String> {
    let mut manifest = request.manifest;
    manifest.validate()?;
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let private_key = private_key_for_address(&secret, &request.public, &manifest.creator)?;
    let signing = manifest.signing_bytes()?;
    manifest.creator_signature =
        sign_domain_message(&private_key, MEDIA_MANIFEST_DOMAIN, &signing)?;
    verify_domain_message(
        &manifest.creator,
        &manifest.creator_signature,
        MEDIA_MANIFEST_DOMAIN,
        &signing,
    )?;
    Ok(manifest)
}
