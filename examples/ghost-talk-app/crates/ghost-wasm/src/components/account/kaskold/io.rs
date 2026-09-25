use base64::{engine::general_purpose::STANDARD, Engine as _};
use wasm_bindgen::JsCast;
use web_sys::{HtmlAnchorElement, HtmlSelectElement};
use yew::prelude::*;

use crate::model::KasKoldBackupResult;

pub(super) fn render_status(
    status: &UseStateHandle<String>,
    result: &UseStateHandle<String>,
) -> Html {
    html! {
        <>
            {if status.is_empty() { Html::default() } else { html! { <p class="status">{(**status).clone()}</p> } }}
            {if result.is_empty() { Html::default() } else { html! { <textarea class="kaskold-result" readonly=true value={(**result).clone()} /> } }}
        </>
    }
}

pub(super) fn show_backup(
    value: KasKoldBackupResult,
    result: &UseStateHandle<String>,
    status: &UseStateHandle<String>,
) {
    if let Some(text) = value.text {
        result.set(text);
        status.set(format!("{} created.", value.filename));
    } else if download_bytes(&value.filename, &value.media_type, &value.bytes).is_ok() {
        status.set(format!("{} downloaded.", value.filename));
    } else {
        result.set(STANDARD.encode(&value.bytes));
        status.set(format!("{} created; base64 shown below.", value.filename));
    }
}

pub(super) fn select_state(state: UseStateHandle<String>) -> Callback<Event> {
    Callback::from(move |event: Event| {
        state.set(event.target_unchecked_into::<HtmlSelectElement>().value());
    })
}

pub(super) fn download_bytes(filename: &str, media_type: &str, bytes: &[u8]) -> Result<(), String> {
    let encoded = STANDARD.encode(bytes);
    let href = format!("data:{media_type};base64,{encoded}");
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("Browser document unavailable")?;
    let anchor = document
        .create_element("a")
        .map_err(|_| "Could not create download link")?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|_| "Could not create download link")?;
    anchor.set_href(&href);
    anchor.set_download(filename);
    anchor.click();
    Ok(())
}
