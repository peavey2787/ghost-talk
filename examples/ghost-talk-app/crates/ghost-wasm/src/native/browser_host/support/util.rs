use serde::de::DeserializeOwned;
use serde_json::Value;

pub(in crate::native::browser_host) fn required_str<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("browser command argument {name} is missing or not a string"))
}

pub(in crate::native::browser_host) fn optional_str<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

pub(in crate::native::browser_host) fn required<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(
        args.get(name)
            .cloned()
            .ok_or_else(|| format!("browser command argument {name} is missing"))?,
    )
    .map_err(|error| format!("browser command argument {name}: {error}"))
}

pub(in crate::native::browser_host) fn to_value<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

pub(in crate::native::browser_host) fn require_password(password: &str, label: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err(format!("{label} password must be at least 8 characters"));
    }
    Ok(())
}

pub(in crate::native::browser_host) fn supported_network(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mainnet" => Ok("mainnet".into()),
        "testnet-10" => Ok("testnet-10".into()),
        _ => Err("Kaspa network must be mainnet or testnet-10".into()),
    }
}


pub(in crate::native::browser_host) fn decimal(value: &str, label: &str) -> Result<u64, String> {
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(format!("{label} must be an unsigned decimal integer"));
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("{label} is outside the supported range"))
}


pub(in crate::native::browser_host) fn open_wallet_secret(
    password: &str,
    sealed: &[u8],
    public: &ghost_kaspa::wallet::WalletPublic,
) -> Result<ghost_kaspa::wallet::WalletSecret, String> {
    let secret = ghost_storage::open_json(password, sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, public)?;
    Ok(secret)
}

pub(in crate::native::browser_host) fn option_endpoint<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.pointer(&format!("/options/{name}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
