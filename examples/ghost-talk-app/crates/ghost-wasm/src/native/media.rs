use crate::model::Profile;
use ghost_api::{KaspaArchivePlan, KaspaArchivePublishResult};
use ghost_media::{GhostMediaManifest, MediaReference};
use serde_json::json;

pub(crate) async fn import_local_media(
    content_type: &str,
    data_base64: &str,
) -> Result<MediaReference, String> {
    super::invoke::invoke(
        "media_import_local",
        json!({ "contentType": content_type, "dataBase64": data_base64 }),
    )
    .await
}

pub(crate) async fn sign_manifest(
    profile: &Profile,
    password: &str,
    manifest: GhostMediaManifest,
) -> Result<GhostMediaManifest, String> {
    let wallet = wallet(profile)?;
    super::invoke::invoke(
        "media_manifest_sign",
        json!({
            "request": {
                "profileId": profile.id,
                "password": password,
                "sealed": wallet.sealed,
                "public": wallet.public,
                "manifest": manifest,
            }
        }),
    )
    .await
}

pub(crate) async fn archive_plan(
    profile: &Profile,
    password: &str,
    reference: &MediaReference,
) -> Result<KaspaArchivePlan, String> {
    let wallet = wallet(profile)?;
    super::invoke::invoke(
        "kaspa_archive_plan",
        json!({
            "request": {
                "profileId": profile.id,
                "password": password,
                "sealed": wallet.sealed,
                "public": wallet.public,
                "dataSize": reference.size,
                "wrpcEndpoint": wallet.wrpc_endpoint,
            }
        }),
    )
    .await
}

pub(crate) async fn archive_publish(
    profile: &Profile,
    password: &str,
    reference: &MediaReference,
    maximum_cost_sompi: u64,
) -> Result<KaspaArchivePublishResult, String> {
    let wallet = wallet(profile)?;
    let verified = super::commands::fetch_verified_media(reference).await?;
    super::invoke::invoke(
        "kaspa_archive_publish",
        json!({
            "request": {
                "profileId": profile.id,
                "password": password,
                "sealed": wallet.sealed,
                "public": wallet.public,
                "contentType": reference.content_type,
                "dataBase64": verified.data_base64,
                "maxCostSompi": maximum_cost_sompi,
                "confirmed": true,
                "wrpcEndpoint": wallet.wrpc_endpoint,
            }
        }),
    )
    .await
}

fn wallet(profile: &Profile) -> Result<&crate::model::WalletRecord, String> {
    profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No Kaspa wallet configured".into())
}
