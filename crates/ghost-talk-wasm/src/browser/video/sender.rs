use std::{cell::RefCell, rc::Rc};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement};

use crate::browser::media_api::{camera_stream, js_error, stop_stream_tracks};

const FRAME_INTERVAL_MS: u32 = 125;
const FRAME_WIDTH: u32 = 320;
const FRAME_HEIGHT: u32 = 240;
const JPEG_QUALITY: f64 = 0.6;
const JPEG_DATA_URL: &str = "data:image/jpeg;base64,";
/// `HTMLMediaElement.HAVE_CURRENT_DATA`: a frame is ready to draw.
const HAVE_CURRENT_DATA: u16 = 2;

/// Camera capture for live calls and Room video.
#[derive(Clone, Default)]
pub struct BrowserVideoSender {
    inner: Rc<RefCell<VideoInner>>,
}

#[derive(Default)]
struct VideoInner {
    capture: Option<Capture>,
    generation: u64,
}

#[derive(Clone)]
struct Capture {
    stream: JsValue,
    video: HtmlVideoElement,
    canvas: HtmlCanvasElement,
}

impl BrowserVideoSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.inner.borrow().capture.is_some()
    }

    /// Open the camera and emit one JPEG frame roughly every 125 ms until
    /// `stop`.
    pub async fn start<F>(&self, on_frame: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>) + 'static,
    {
        if self.is_running() {
            return Err("Camera is already on.".into());
        }
        let capture = open_capture().await?;
        let generation = {
            let mut inner = self.inner.borrow_mut();
            inner.generation = inner.generation.wrapping_add(1);
            inner.capture = Some(capture);
            inner.generation
        };
        spawn_local(frame_loop(
            self.inner.clone(),
            generation,
            Rc::new(on_frame),
        ));
        Ok(())
    }

    pub fn stop(&self) {
        let capture = {
            let mut inner = self.inner.borrow_mut();
            inner.generation = inner.generation.wrapping_add(1);
            inner.capture.take()
        };
        if let Some(capture) = capture {
            capture.video.set_src_object(None);
            stop_stream_tracks(&capture.stream);
        }
    }
}

async fn open_capture() -> Result<Capture, String> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "document is unavailable".to_string())?;
    let stream = camera_stream(FRAME_WIDTH, FRAME_HEIGHT).await?;
    let video = create::<HtmlVideoElement>(&document, "video")?;
    video.set_muted(true);
    video.set_attribute("playsinline", "").map_err(js_error)?;
    video.set_src_object(Some(stream.unchecked_ref()));
    let canvas = create::<HtmlCanvasElement>(&document, "canvas")?;
    let playing = video.play().map_err(js_error)?;
    if let Err(error) = JsFuture::from(playing).await {
        stop_stream_tracks(&stream);
        return Err(js_error(error));
    }
    Ok(Capture {
        stream,
        video,
        canvas,
    })
}

fn create<T: JsCast>(document: &web_sys::Document, tag: &str) -> Result<T, String> {
    document
        .create_element(tag)
        .map_err(js_error)?
        .dyn_into::<T>()
        .map_err(|_| format!("<{tag}> is unavailable"))
}

async fn frame_loop(
    inner: Rc<RefCell<VideoInner>>,
    generation: u64,
    on_frame: Rc<dyn Fn(Vec<u8>)>,
) {
    loop {
        TimeoutFuture::new(FRAME_INTERVAL_MS).await;
        let Some(capture) = active_capture(&inner, generation) else {
            return;
        };
        if let Some(frame) = capture_frame(&capture) {
            on_frame(frame);
        }
    }
}

fn active_capture(inner: &Rc<RefCell<VideoInner>>, generation: u64) -> Option<Capture> {
    let state = inner.borrow();
    (state.generation == generation)
        .then(|| state.capture.clone())
        .flatten()
}

fn capture_frame(capture: &Capture) -> Option<Vec<u8>> {
    let video = &capture.video;
    if video.ready_state() < HAVE_CURRENT_DATA || video.video_width() == 0 {
        return None;
    }
    let (width, height) = frame_size(video.video_width(), video.video_height());
    let canvas = &capture.canvas;
    canvas.set_width(width);
    canvas.set_height(height);
    let context = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()?;
    context
        .draw_image_with_html_video_element_and_dw_and_dh(
            video,
            0.0,
            0.0,
            f64::from(width),
            f64::from(height),
        )
        .ok()?;
    let url = canvas
        .to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(JPEG_QUALITY))
        .ok()?;
    STANDARD.decode(url.strip_prefix(JPEG_DATA_URL)?).ok()
}

/// Fit the camera's aspect ratio inside the frame bounds.
fn frame_size(source_width: u32, source_height: u32) -> (u32, u32) {
    let scale = f64::min(
        f64::from(FRAME_WIDTH) / f64::from(source_width),
        f64::from(FRAME_HEIGHT) / f64::from(source_height.max(1)),
    )
    .min(1.0);
    let scaled = |value: u32| ((f64::from(value) * scale).round() as u32).max(1);
    (scaled(source_width), scaled(source_height))
}

#[cfg(test)]
mod tests {
    use super::frame_size;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn frames_fit_the_bounds_and_keep_the_aspect_ratio() {
        assert_eq!(frame_size(1280, 720), (320, 180));
        assert_eq!(frame_size(640, 480), (320, 240));
        assert_eq!(frame_size(480, 640), (180, 240));
        assert_eq!(frame_size(160, 120), (160, 120));
    }
}
