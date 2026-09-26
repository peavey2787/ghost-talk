use serde_json::Value;

use super::util::required_str;

const PROFILE_STATE_KEY: &str = "ghost-talk.profile-state.v1";

pub(in crate::native::browser_host) fn invoke(
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    match command {
        "profile_state_load" => profile_state_load(),
        "profile_state_save" => profile_state_save(args),
        _ => Err(format!("unknown browser profile command: {command}")),
    }
}

fn profile_state_load() -> Result<Value, String> {
    let storage = local_storage()?;
    storage
        .get_item(PROFILE_STATE_KEY)
        .map(|value| value.map(Value::String).unwrap_or(Value::Null))
        .map_err(|error| js_error("read browser profile state", error))
}

fn profile_state_save(args: &Value) -> Result<Value, String> {
    let json = required_str(args, "json")?;
    local_storage()?
        .set_item(PROFILE_STATE_KEY, json)
        .map_err(|error| js_error("write browser profile state", error))?;
    Ok(Value::Null)
}

pub(in crate::native::browser_host) fn local_storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or_else(|| "window is unavailable".to_string())?
        .local_storage()
        .map_err(|error| js_error("open browser local storage", error))?
        .ok_or_else(|| "browser local storage is unavailable for this origin".to_string())
}

fn js_error(action: &str, value: wasm_bindgen::JsValue) -> String {
    let detail = value
        .as_string()
        .or_else(|| {
            js_sys::JSON::stringify(&value)
                .ok()
                .and_then(|text| text.as_string())
        })
        .unwrap_or_else(|| "JavaScript error".into());
    format!("could not {action}: {detail}")
}
