use crate::model::{Chat, IncomingRequest, Profile, ProfilePatch};

/// Accept an incoming call bootstrap through the canonical chat/contact command
/// and return the exact local profile projection plus its typed persistence delta.
pub(crate) async fn accept_bootstrap(
    profile: &Profile,
    password: &str,
    chat: &Chat,
    request: &IncomingRequest,
) -> Result<(Profile, ProfilePatch), String> {
    let patch = crate::controllers::chat::accept_request(profile, password, chat, request).await?;
    let updated = crate::app::apply_profile_patch(profile, patch.clone())?;
    Ok((updated, patch))
}
