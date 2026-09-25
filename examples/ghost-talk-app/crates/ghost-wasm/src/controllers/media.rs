use crate::model::{Profile, ProfilePatch, WalletStateService};
use ghost_api::{KaspaArchivePlan, VerifiedMedia};
use ghost_media::{manifest_for_reference, GhostMediaManifest, MediaReference};

pub async fn fetch_verified(reference: &MediaReference) -> Result<VerifiedMedia, String> {
    crate::native::fetch_verified_media(reference).await
}

pub(crate) async fn import_local_media(
    content_type: &str,
    data_base64: &str,
) -> Result<MediaReference, String> {
    crate::native::import_local_media(content_type, data_base64).await
}

pub(crate) async fn archive_plan(
    profile: &Profile,
    password: &str,
    reference: &MediaReference,
) -> Result<KaspaArchivePlan, String> {
    crate::native::archive_plan(profile, password, reference).await
}

pub(crate) struct ArchiveOutcome {
    pub(crate) reference: MediaReference,
    pub(crate) patch: ProfilePatch,
    pub(crate) transaction_count: usize,
    pub(crate) actual_fee_sompi: String,
    pub(crate) fee_is_exact: bool,
}

pub(crate) async fn archive_publish(
    profile: &Profile,
    password: &str,
    reference: &MediaReference,
    maximum_cost_sompi: u64,
) -> Result<ArchiveOutcome, String> {
    let result =
        crate::native::archive_publish(profile, password, reference, maximum_cost_sompi).await?;
    let mut wallet = profile.wallet.clone();
    WalletStateService::merge_progress(&mut wallet, result.public.public);
    let mut patch = ProfilePatch::new(&profile.id);
    patch.wallet(profile.wallet.clone(), wallet);
    Ok(ArchiveOutcome {
        reference: result.reference,
        patch,
        transaction_count: result.transaction_ids.len(),
        actual_fee_sompi: result.actual_fee_sompi,
        fee_is_exact: result.fee_is_exact,
    })
}

pub(crate) async fn sign_recording_manifest(
    profile: &Profile,
    password: &str,
    reference: &MediaReference,
    title: &str,
    duration_ms: u64,
) -> Result<GhostMediaManifest, String> {
    let creator = profile
        .wallet
        .as_ref()
        .and_then(|wallet| wallet.public.receive_addresses.first())
        .cloned()
        .ok_or_else(|| "Kaspa creator address is unavailable".to_string())?;
    let manifest = manifest_for_reference(
        reference,
        creator,
        title.trim().to_owned(),
        codec_for(reference),
        duration_ms,
    );
    crate::native::sign_manifest(profile, password, manifest).await
}

fn codec_for(reference: &MediaReference) -> String {
    let content_type = reference.content_type.to_ascii_lowercase();
    if content_type.contains("opus")
        || content_type.contains("webm")
        || content_type.contains("ogg")
    {
        "opus".into()
    } else {
        "unknown".into()
    }
}
