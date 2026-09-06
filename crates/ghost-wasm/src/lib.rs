#![forbid(unsafe_code)]
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ghost_version() -> String {
    format!("{} {}", ghost_core::APP_NAME, ghost_core::APP_VERSION)
}
