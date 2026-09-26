use js_sys::Uint8Array;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement, SubmitEvent};
use yew::prelude::*;

pub(crate) fn text_input(state: UseStateHandle<String>) -> Callback<InputEvent> {
    Callback::from(move |event: InputEvent| {
        state.set(event.target_unchecked_into::<HtmlInputElement>().value());
    })
}

pub(crate) fn textarea_input(state: UseStateHandle<String>) -> Callback<InputEvent> {
    Callback::from(move |event: InputEvent| {
        state.set(event.target_unchecked_into::<HtmlTextAreaElement>().value());
    })
}

pub(crate) fn checkbox_input(state: UseStateHandle<bool>) -> Callback<Event> {
    Callback::from(move |event: Event| {
        state.set(event.target_unchecked_into::<HtmlInputElement>().checked());
    })
}

pub(crate) fn select_value(on_value: Callback<String>) -> Callback<Event> {
    Callback::from(move |event: Event| {
        on_value.emit(event.target_unchecked_into::<HtmlSelectElement>().value());
    })
}

pub(crate) fn binary_file_input(
    state: UseStateHandle<Vec<u8>>,
    status: UseStateHandle<String>,
) -> Callback<Event> {
    Callback::from(move |event: Event| {
        let input = event.target_unchecked_into::<HtmlInputElement>();
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        let state = state.clone();
        let status = status.clone();
        spawn_local(async move {
            match JsFuture::from(file.array_buffer()).await {
                Ok(buffer) => {
                    let bytes = Uint8Array::new(&buffer).to_vec();
                    status.set(format!("Loaded {} bytes.", bytes.len()));
                    state.set(bytes);
                }
                Err(_) => status.set("Could not read selected file.".into()),
            }
        });
    })
}

pub(crate) fn prevent_submit() -> Callback<SubmitEvent> {
    Callback::from(|event: SubmitEvent| event.prevent_default())
}

pub(crate) fn status_view(status: &UseStateHandle<String>) -> Html {
    if status.is_empty() {
        Html::default()
    } else {
        html! { <div class="status floating-status">{(**status).clone()}</div> }
    }
}
