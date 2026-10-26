use ghost_api::VerifiedMedia;
use ghost_media::MediaReference;

pub async fn fetch_verified(reference: &MediaReference) -> Result<VerifiedMedia, String> {
    crate::native::fetch_verified_media(reference).await
}

pub(crate) async fn import_local_media(
    content_type: &str,
    data_base64: &str,
) -> Result<MediaReference, String> {
    crate::native::import_local_media(content_type, data_base64).await
}
