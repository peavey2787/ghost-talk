use ghost_media::MediaReference;
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
