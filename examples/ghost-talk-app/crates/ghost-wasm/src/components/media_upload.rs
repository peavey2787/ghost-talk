use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_media::MediaReference;
use js_sys::Uint8Array;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MediaUploadProps {
    pub label: String,
    pub accept: String,
    pub content_type_prefix: String,
    pub on_media: Callback<MediaReference>,
    pub on_error: Callback<String>,
}

#[component(MediaUpload)]
pub fn media_upload(props: &MediaUploadProps) -> Html {
    let busy = use_state(|| false);
    let onchange = upload_callback(props, busy.clone());
    html! {
        <label class="media-upload">
            <span>{props.label.clone()}</span>
            <input type="file" accept={props.accept.clone()} disabled={*busy} {onchange} />
            {if *busy {
                html! { <small>{"Importing and hashing…"}</small> }
            } else {
                Html::default()
            }}
        </label>
    }
}

fn upload_callback(props: &MediaUploadProps, busy: UseStateHandle<bool>) -> Callback<Event> {
    let prefix = props.content_type_prefix.clone();
    let on_media = props.on_media.clone();
    let on_error = props.on_error.clone();
    Callback::from(move |event: Event| {
        let input: HtmlInputElement = event.target_unchecked_into();
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        let content_type = file.type_();
        if !content_type.starts_with(&prefix) {
            on_error.emit(format!("Selected media must use a {prefix} content type."));
            return;
        }
        busy.set(true);
        let busy = busy.clone();
        let on_media = on_media.clone();
        let on_error = on_error.clone();
        spawn_local(async move {
            match import_file(file, content_type).await {
                Ok(reference) => on_media.emit(reference),
                Err(error) => on_error.emit(error),
            }
            busy.set(false);
        });
    })
}

async fn import_file(file: web_sys::File, content_type: String) -> Result<MediaReference, String> {
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "Could not read the selected media file.".to_string())?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    if bytes.is_empty() {
        return Err("Selected media file is empty.".into());
    }
    let encoded = STANDARD.encode(bytes);
    crate::controllers::media::import_local_media(&content_type, &encoded).await
}
