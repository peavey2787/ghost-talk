use serde::Serialize;

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut out = String::new();
    write_canonical_json(&value, &mut out)?;
    Ok(out.into_bytes())
}

pub(crate) fn write_canonical_json(
    value: &serde_json::Value,
    out: &mut String,
) -> Result<(), String> {
    match value {
        serde_json::Value::Array(values) => write_canonical_array(values, out),
        serde_json::Value::Object(values) => write_canonical_object(values, out),
        _ => write_canonical_scalar(value, out),
    }
}

fn write_canonical_scalar(value: &serde_json::Value, out: &mut String) -> Result<(), String> {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        serde_json::Value::Number(value) => out.push_str(&value.to_string()),
        serde_json::Value::String(value) => {
            out.push_str(&serde_json::to_string(value).map_err(|e| e.to_string())?)
        }
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            return Err("canonical JSON scalar writer received a collection".into());
        }
    }
    Ok(())
}

fn write_canonical_array(values: &[serde_json::Value], out: &mut String) -> Result<(), String> {
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_canonical_json(value, out)?;
    }
    out.push(']');
    Ok(())
}

fn write_canonical_object(
    values: &serde_json::Map<String, serde_json::Value>,
    out: &mut String,
) -> Result<(), String> {
    out.push('{');
    let mut keys: Vec<_> = values.keys().collect();
    keys.sort_unstable();
    for (index, key) in keys.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(key).map_err(|e| e.to_string())?);
        out.push(':');
        write_canonical_json(&values[key], out)?;
    }
    out.push('}');
    Ok(())
}
