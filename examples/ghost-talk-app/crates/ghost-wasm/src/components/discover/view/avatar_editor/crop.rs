use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_media::MediaReference;
use gloo_timers::future::TimeoutFuture;
use js_sys::Uint8Array;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};

const CROP_SIZE: u32 = 256;

#[derive(Clone, PartialEq)]
pub(super) struct AvatarSource {
    pub(super) data_url: String,
    pub(super) width: f64,
    pub(super) height: f64,
}

pub(super) async fn avatar_source(file: web_sys::File) -> Result<AvatarSource, String> {
    let content_type = file.type_();
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(js_error)?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    if bytes.is_empty() {
        return Err("Selected avatar image is empty.".into());
    }
    let data_url = format!("data:{content_type};base64,{}", STANDARD.encode(bytes));
    let image = load_image(&data_url).await?;
    Ok(AvatarSource {
        data_url,
        width: f64::from(image.natural_width()),
        height: f64::from(image.natural_height()),
    })
}

pub(super) async fn crop_and_import(
    source: &str,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
) -> Result<MediaReference, String> {
    let image = load_image(source).await?;
    let canvas = canvas()?;
    let context = canvas_context(&canvas)?;
    let (dx, dy, width, height) = crop_geometry(&image, zoom, offset_x, offset_y)?;
    context
        .draw_image_with_html_image_element_and_dw_and_dh(&image, dx, dy, width, height)
        .map_err(js_error)?;
    let data_url = canvas
        .to_data_url_with_type("image/png")
        .map_err(js_error)?;
    let encoded = data_url
        .split_once(',')
        .map(|(_, body)| body)
        .ok_or_else(|| "Browser canvas returned an invalid PNG data URL".to_string())?;
    crate::controllers::media::import_local_media("image/png", encoded).await
}

pub(super) fn geometry(
    source_width: f64,
    source_height: f64,
    size: f64,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
) -> (f64, f64, f64, f64) {
    let scale = (size / source_width).max(size / source_height) * zoom.clamp(1.0, 3.0);
    let width = source_width * scale;
    let height = source_height * scale;
    let pan_x = ((width - size) / 2.0).max(0.0);
    let pan_y = ((height - size) / 2.0).max(0.0);
    let left = (size - width) / 2.0 + offset_x.clamp(-100.0, 100.0) / 100.0 * pan_x;
    let top = (size - height) / 2.0 + offset_y.clamp(-100.0, 100.0) / 100.0 * pan_y;
    (left, top, width, height)
}

fn crop_geometry(
    image: &HtmlImageElement,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
) -> Result<(f64, f64, f64, f64), String> {
    let width = f64::from(image.natural_width());
    let height = f64::from(image.natural_height());
    if width <= 0.0 || height <= 0.0 {
        return Err("Avatar image has invalid dimensions.".into());
    }
    Ok(geometry(
        width,
        height,
        f64::from(CROP_SIZE),
        zoom,
        offset_x,
        offset_y,
    ))
}

async fn load_image(source: &str) -> Result<HtmlImageElement, String> {
    let image = HtmlImageElement::new().map_err(js_error)?;
    image.set_src(source);
    for _ in 0..100 {
        if image.complete() && image.natural_width() > 0 && image.natural_height() > 0 {
            return Ok(image);
        }
        TimeoutFuture::new(20).await;
    }
    Err("Avatar image did not finish decoding in the browser.".into())
}

fn canvas() -> Result<HtmlCanvasElement, String> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "Browser document unavailable".to_string())?;
    let canvas = document
        .create_element("canvas")
        .map_err(js_error)?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| "Browser could not create an avatar canvas".to_string())?;
    canvas.set_width(CROP_SIZE);
    canvas.set_height(CROP_SIZE);
    Ok(canvas)
}

fn canvas_context(canvas: &HtmlCanvasElement) -> Result<CanvasRenderingContext2d, String> {
    canvas
        .get_context("2d")
        .map_err(js_error)?
        .ok_or_else(|| "Browser 2D canvas is unavailable".to_string())?
        .dyn_into::<CanvasRenderingContext2d>()
        .map_err(|_| "Browser 2D canvas context is invalid".to_string())
}

fn js_error(value: JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| "Browser image operation failed".into())
}
