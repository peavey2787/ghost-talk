#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;

/// Read one JavaScript property through the browser reflection boundary.
///
/// Callers keep operation-specific conversion errors, while this helper keeps
/// raw property access DRY and consistently maps opaque JS exceptions.
pub(crate) fn get(target: &JsValue, name: &str) -> Result<JsValue, String> {
    ghost_talk_wasm::browser_property(target, name).map_err(|value| {
        value
            .as_string()
            .unwrap_or_else(|| "Browser property access failed".into())
    })
}
