mod crop;

use crop::{avatar_source, crop_and_import, geometry, AvatarSource};
use ghost_media::MediaReference;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

const PREVIEW_SIZE: f64 = 160.0;

type DragCallbacks = (
    Callback<MouseEvent>,
    Callback<MouseEvent>,
    Callback<MouseEvent>,
);

#[derive(Properties, PartialEq)]
pub(super) struct AvatarCropEditorProps {
    pub label: String,
    pub current: Option<MediaReference>,
    pub on_media: Callback<MediaReference>,
    pub on_error: Callback<String>,
}

#[component(AvatarCropEditor)]
pub(super) fn avatar_crop_editor(props: &AvatarCropEditorProps) -> Html {
    let source = use_state(|| None::<AvatarSource>);
    let zoom = use_state(|| 1.0_f64);
    let offset_x = use_state(|| 0.0_f64);
    let offset_y = use_state(|| 0.0_f64);
    let dragging = use_state(|| None::<(i32, i32)>);
    let busy = use_state(|| false);
    let file_change = file_callback(
        props,
        source.clone(),
        zoom.clone(),
        offset_x.clone(),
        offset_y.clone(),
    );
    let apply = apply_callback(
        props,
        source.clone(),
        zoom.clone(),
        offset_x.clone(),
        offset_y.clone(),
        busy.clone(),
    );
    let drag = drag_callbacks(offset_x.clone(), offset_y.clone(), dragging);
    html! {
        <div class="avatar-crop-editor">
            <div class="avatar-crop-preview" onmousedown={drag.0} onmousemove={drag.1} onmouseup={drag.2.clone()} onmouseleave={drag.2}>
                {preview(props, (*source).as_ref(), *zoom, *offset_x, *offset_y)}
            </div>
            <div class="avatar-crop-controls">
                <label class="media-upload"><span>{"Profile avatar"}</span><input type="file" accept="image/*" onchange={file_change}/></label>
                {controls((*source).is_some(), *busy, zoom, offset_x, offset_y, apply)}
            </div>
        </div>
    }
}

fn controls(
    has_source: bool,
    busy: bool,
    zoom: UseStateHandle<f64>,
    offset_x: UseStateHandle<f64>,
    offset_y: UseStateHandle<f64>,
    apply: Callback<MouseEvent>,
) -> Html {
    if !has_source {
        return html! { <small>{"Choose an image to preview, reposition, and crop it before publishing."}</small> };
    }
    let reset = reset_callback(zoom.clone(), offset_x.clone(), offset_y.clone());
    html! {
        <>
            {range_control("Zoom", 100.0, 300.0, *zoom * 100.0, zoom, 0.01)}
            {range_control("Horizontal", -100.0, 100.0, *offset_x, offset_x, 1.0)}
            {range_control("Vertical", -100.0, 100.0, *offset_y, offset_y, 1.0)}
            <div class="button-row">
                <button type="button" class="primary" disabled={busy} onclick={apply}>{if busy { "Applying…" } else { "Apply crop" }}</button>
                <button type="button" disabled={busy} onclick={reset}>{"Reset"}</button>
            </div>
            <small>{"Drag the preview or use the controls to position the final square avatar."}</small>
        </>
    }
}

fn preview(
    props: &AvatarCropEditorProps,
    source: Option<&AvatarSource>,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
) -> Html {
    if let Some(source) = source {
        let (left, top, width, height) = geometry(
            source.width,
            source.height,
            PREVIEW_SIZE,
            zoom,
            offset_x,
            offset_y,
        );
        let style = format!(
            "left:{left:.3}px;top:{top:.3}px;width:{width:.3}px;height:{height:.3}px"
        );
        return html! { <img class="avatar-crop-image" src={source.data_url.clone()} {style} alt="Avatar crop preview" draggable="false"/> };
    }
    html! { <crate::components::avatar::Avatar label={props.label.clone()} reference={props.current.clone()} size={160}/> }
}

fn file_callback(
    props: &AvatarCropEditorProps,
    source: UseStateHandle<Option<AvatarSource>>,
    zoom: UseStateHandle<f64>,
    offset_x: UseStateHandle<f64>,
    offset_y: UseStateHandle<f64>,
) -> Callback<Event> {
    let on_error = props.on_error.clone();
    Callback::from(move |event: Event| {
        let input: HtmlInputElement = event.target_unchecked_into();
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        if !file.type_().starts_with("image/") {
            on_error.emit("Selected avatar must be an image.".into());
            return;
        }
        load_file(
            file,
            source.clone(),
            zoom.clone(),
            offset_x.clone(),
            offset_y.clone(),
            on_error.clone(),
        );
    })
}

fn load_file(
    file: web_sys::File,
    source: UseStateHandle<Option<AvatarSource>>,
    zoom: UseStateHandle<f64>,
    offset_x: UseStateHandle<f64>,
    offset_y: UseStateHandle<f64>,
    on_error: Callback<String>,
) {
    spawn_local(async move {
        match avatar_source(file).await {
            Ok(value) => {
                source.set(Some(value));
                zoom.set(1.0);
                offset_x.set(0.0);
                offset_y.set(0.0);
            }
            Err(error) => on_error.emit(error),
        }
    });
}

fn range_control(
    label: &'static str,
    min: f64,
    max: f64,
    value: f64,
    state: UseStateHandle<f64>,
    scale: f64,
) -> Html {
    let input_state = state.clone();
    let oninput = Callback::from(move |event: InputEvent| {
        let input: HtmlInputElement = event.target_unchecked_into();
        set_range_state(&input_state, input.value_as_number(), scale);
    });
    let change_state = state;
    let onchange = Callback::from(move |event: Event| {
        let input: HtmlInputElement = event.target_unchecked_into();
        set_range_state(&change_state, input.value_as_number(), scale);
    });
    html! {
        <label class="avatar-range">
            <span>{label}</span>
            <input type="range" min={min.to_string()} max={max.to_string()} step="1" value={value.to_string()} {oninput} {onchange}/>
            <output>{format!("{value:.0}%")}</output>
        </label>
    }
}

fn set_range_state(state: &UseStateHandle<f64>, raw: f64, scale: f64) {
    if raw.is_finite() {
        state.set(raw * scale);
    }
}

fn reset_callback(
    zoom: UseStateHandle<f64>,
    offset_x: UseStateHandle<f64>,
    offset_y: UseStateHandle<f64>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        zoom.set(1.0);
        offset_x.set(0.0);
        offset_y.set(0.0);
    })
}

fn drag_callbacks(
    offset_x: UseStateHandle<f64>,
    offset_y: UseStateHandle<f64>,
    dragging: UseStateHandle<Option<(i32, i32)>>,
) -> DragCallbacks {
    let down_state = dragging.clone();
    let down = Callback::from(move |event: MouseEvent| {
        event.prevent_default();
        down_state.set(Some((event.client_x(), event.client_y())));
    });
    let move_state = dragging.clone();
    let move_x = offset_x.clone();
    let move_y = offset_y.clone();
    let movement = Callback::from(move |event: MouseEvent| {
        let Some((last_x, last_y)) = *move_state else {
            return;
        };
        let next_x = (*move_x + f64::from(event.client_x() - last_x) / 1.6).clamp(-100.0, 100.0);
        let next_y = (*move_y + f64::from(event.client_y() - last_y) / 1.6).clamp(-100.0, 100.0);
        move_x.set(next_x);
        move_y.set(next_y);
        move_state.set(Some((event.client_x(), event.client_y())));
    });
    let up = Callback::from(move |_| dragging.set(None));
    (down, movement, up)
}

fn apply_callback(
    props: &AvatarCropEditorProps,
    source: UseStateHandle<Option<AvatarSource>>,
    zoom: UseStateHandle<f64>,
    offset_x: UseStateHandle<f64>,
    offset_y: UseStateHandle<f64>,
    busy: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    let on_media = props.on_media.clone();
    let on_error = props.on_error.clone();
    Callback::from(move |_| {
        let Some(source) = (*source).as_ref().cloned() else {
            return;
        };
        busy.set(true);
        apply_crop(
            source,
            *zoom,
            *offset_x,
            *offset_y,
            busy.clone(),
            on_media.clone(),
            on_error.clone(),
        );
    })
}

fn apply_crop(
    source: AvatarSource,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
    busy: UseStateHandle<bool>,
    on_media: Callback<MediaReference>,
    on_error: Callback<String>,
) {
    spawn_local(async move {
        match crop_and_import(&source.data_url, zoom, offset_x, offset_y).await {
            Ok(reference) => on_media.emit(reference),
            Err(error) => on_error.emit(error),
        }
        busy.set(false);
    });
}
