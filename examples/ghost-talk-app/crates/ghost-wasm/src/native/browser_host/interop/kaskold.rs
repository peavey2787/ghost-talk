use serde_json::Value;

use super::super::support::util::{required, required_str, to_value};

pub(in crate::native::browser_host) fn invoke(
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    match command {
        "kaskold_import_text" => import_text(args),
        "kaskold_import_bytes" => import_bytes(args),
        "kaskold_backup" => backup(args),
        _ => invoke_signing(command, args),
    }
}

fn invoke_signing(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "kaskold_review_pskt" => review_pskt(args),
        "kaskold_sign_pskt" => sign_pskt(args),
        _ => Err(format!("unknown browser KasKold command: {command}")),
    }
}

fn import_text(args: &Value) -> Result<Value, String> {
    let sealed: Vec<u8> = required(args, "sealedInventory")?;
    to_value(ghost_kaskold::import_text_inventory(
        required_str(args, "password")?,
        &sealed,
        required_str(args, "kind")?,
        required_str(args, "value")?,
        args.get("passphrase")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?)
}

fn import_bytes(args: &Value) -> Result<Value, String> {
    let sealed: Vec<u8> = required(args, "sealedInventory")?;
    let data: Vec<u8> = required(args, "data")?;
    to_value(ghost_kaskold::import_bytes_inventory(
        required_str(args, "password")?,
        &sealed,
        required_str(args, "kind")?,
        &data,
        args.get("credential")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?)
}

fn backup(args: &Value) -> Result<Value, String> {
    let sealed: Vec<u8> = required(args, "sealedInventory")?;
    let carrier: Vec<u8> = args
        .get("carrier")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    to_value(ghost_kaskold::backup_inventory(
        required_str(args, "password")?,
        &sealed,
        required_str(args, "kind")?,
        args.get("credential")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &carrier,
    )?)
}

fn review_pskt(args: &Value) -> Result<Value, String> {
    let sealed: Vec<u8> = required(args, "sealedInventory")?;
    to_value(ghost_kaskold::review_pskt(
        required_str(args, "password")?,
        &sealed,
        required_str(args, "psktHex")?,
        required_str(args, "network")?,
    )?)
}

fn sign_pskt(args: &Value) -> Result<Value, String> {
    let sealed: Vec<u8> = required(args, "sealedInventory")?;
    to_value(ghost_kaskold::sign_pskt(
        required_str(args, "password")?,
        &sealed,
        required_str(args, "psktHex")?,
        required_str(args, "network")?,
        required_str(args, "reviewToken")?,
    )?)
}
