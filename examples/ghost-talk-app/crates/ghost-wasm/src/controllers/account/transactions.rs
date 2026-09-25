use crate::model::{Profile, ProfilePatch};

pub(crate) struct BroadcastPatch {
    pub(crate) transaction_id: String,
    pub(crate) patch: ProfilePatch,
}

pub(crate) async fn send_kaspa_and_patch(
    profile: &Profile,
    password: &str,
    destination: &str,
    amount_sompi: &str,
    reuse_unlocked: bool,
) -> Result<BroadcastPatch, String> {
    let resolved_destination = resolve_destination(profile, password, destination).await?;
    let result = crate::native::send_kaspa(
        profile,
        password,
        &resolved_destination,
        amount_sompi,
        "0",
        reuse_unlocked,
    )
    .await?;
    Ok(broadcast_patch(profile, result))
}

pub(crate) async fn prepare_signer_send(
    profile: &Profile,
    password: &str,
    destination: &str,
    amount_sompi: &str,
) -> Result<String, String> {
    let resolved_destination = resolve_destination(profile, password, destination).await?;
    crate::native::prepare_signer_send(profile, &resolved_destination, amount_sompi, "0").await
}

pub(crate) async fn broadcast_signer_send_and_patch(
    profile: &Profile,
    signed_pskt: &str,
) -> Result<BroadcastPatch, String> {
    let result = crate::native::broadcast_signer_send(profile, signed_pskt).await?;
    Ok(broadcast_patch(profile, result))
}

pub(crate) async fn consolidate_and_patch(
    profile: &Profile,
    password: &str,
) -> Result<BroadcastPatch, String> {
    let result = crate::native::consolidate_kaspa(profile, password, "0", true).await?;
    Ok(broadcast_patch(profile, result))
}

async fn resolve_destination(
    profile: &Profile,
    password: &str,
    destination: &str,
) -> Result<String, String> {
    if destination.to_ascii_lowercase().starts_with("kaspa") {
        Ok(destination.to_string())
    } else {
        Ok(crate::native::resolve_peer(profile, password, destination)
            .await?
            .kaspa_address)
    }
}

fn broadcast_patch(profile: &Profile, result: crate::model::BroadcastResult) -> BroadcastPatch {
    BroadcastPatch {
        transaction_id: result.transaction_id,
        patch: super::state::wallet_progress_patch(profile, result.public),
    }
}
