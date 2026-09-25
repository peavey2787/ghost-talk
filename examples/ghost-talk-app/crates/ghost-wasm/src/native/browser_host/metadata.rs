use serde_json::{to_value, Value};

pub(super) fn invoke(command: &str) -> Result<Value, String> {
    let value = match command {
        "app_info" => to_value(ghost_api::app_info()),
        "derivation_presets" => to_value(ghost_api::derivation_presets()),
        _ => return Err(format!("unsupported browser metadata command: {command}")),
    };
    value.map_err(|error| error.to_string())
}
