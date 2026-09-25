use crate::model::{Profile, ProfilePatch};
use yew::prelude::*;

/// Shared properties for profile-scoped UI that may persist typed profile deltas.
#[derive(Properties, PartialEq)]
pub struct AuthenticatedProfileProps {
    pub profile: Profile,
    pub password: String,
    pub on_update: Callback<ProfilePatch>,
}
