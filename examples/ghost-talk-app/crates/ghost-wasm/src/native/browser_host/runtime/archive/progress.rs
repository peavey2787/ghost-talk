use ghost_kaspa::ArchiveProgress;

const KEY_PREFIX: &str = "ghost-talk.archive-progress.v1.";

pub(in crate::native::browser_host) fn load(
    profile_id: &str,
    media_id: &str,
) -> Result<Option<ArchiveProgress>, String> {
    let Some(encoded) = super::super::super::support::storage::local_storage()?
        .get_item(&key(profile_id, media_id))
        .map_err(crate::native::invoke::js_error)?
    else {
        return Ok(None);
    };
    serde_json::from_str(&encoded)
        .map(Some)
        .map_err(|error| format!("decode archive progress: {error}"))
}

pub(in crate::native::browser_host) fn save(value: &ArchiveProgress) -> Result<(), String> {
    let encoded = serde_json::to_string(value)
        .map_err(|error| format!("encode archive progress: {error}"))?;
    super::super::super::support::storage::local_storage()?
        .set_item(&key(&value.profile_id, &value.media_id), &encoded)
        .map_err(crate::native::invoke::js_error)
}

pub(in crate::native::browser_host) fn remove(
    profile_id: &str,
    media_id: &str,
) -> Result<(), String> {
    super::super::super::support::storage::local_storage()?
        .remove_item(&key(profile_id, media_id))
        .map_err(crate::native::invoke::js_error)
}

fn key(profile_id: &str, media_id: &str) -> String {
    format!("{KEY_PREFIX}{profile_id}.{media_id}")
}
