use crate::model::{Profile, ProfilePatch, PublicGhostProfile, PublishedGhostDescriptor};

pub(crate) async fn lookup_public_profile(
    profile: &Profile,
    target: &str,
) -> Result<Option<PublicGhostProfile>, String> {
    crate::native::lookup_public_profile(profile, target).await
}

pub(crate) async fn publish_descriptor(
    profile: &Profile,
    password: &str,
    discoverable: bool,
) -> Result<PublishedGhostDescriptor, String> {
    crate::native::publish_descriptor(profile, password, discoverable).await
}

pub(crate) fn update_public_text(
    profile: &Profile,
    field: &str,
    value: String,
) -> Result<ProfilePatch, String> {
    let mut settings = profile.settings.clone();
    match field {
        "username" => settings.public_username = value,
        "description" => settings.public_description = value,
        "interests" => settings.public_interests = value,
        _ => return Err(format!("Unknown public profile field: {field}")),
    }
    let mut patch = ProfilePatch::new(&profile.id);
    patch.settings(profile.settings.clone(), settings);
    Ok(patch)
}

pub(crate) fn update_public_avatar(
    profile: &Profile,
    avatar: ghost_media::MediaReference,
) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&profile.id);
    patch.public_avatar(Some(avatar));
    patch
}

pub(crate) async fn publish_and_patch(
    profile: &Profile,
    password: &str,
    discoverable: bool,
) -> Result<(PublishedGhostDescriptor, ProfilePatch), String> {
    let published = publish_descriptor(profile, password, discoverable).await?;
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::set_registered_address(
        &mut wallet,
        published.kaspa_address.clone(),
        published.public.clone(),
    );
    let mut patch = ProfilePatch::new(&profile.id);
    patch.wallet(profile.wallet.clone(), wallet);
    Ok((published, patch))
}
