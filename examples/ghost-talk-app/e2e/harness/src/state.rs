//! Persistent E2E wallets: one dev (funding) wallet plus the two instance
//! identities. The file holds recovery words, so it lives outside the repo.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const ACCOUNT_PATH: &str = "m/44'/111111'/0'";

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallets {
    pub network: String,
    pub dev: String,
    pub instance_a: String,
    pub instance_b: String,
}

pub fn home() -> PathBuf {
    if let Ok(path) = std::env::var("GHOST_TALK_E2E_HOME") {
        return PathBuf::from(path);
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".ghost-talk").join("e2e")
}

fn file(network: &str) -> PathBuf {
    home().join(format!("dev-wallet-{network}.json"))
}

/// Load the wallets, creating and persisting fresh ones on first use.
pub fn load_or_create(network: &str) -> Result<Wallets, String> {
    let path = file(network);
    if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        return serde_json::from_str(&raw).map_err(|error| {
            format!("{} is not a valid E2E wallet file: {error}", path.display())
        });
    }
    let generate = || {
        ghost_kaspa::wallet::generate_wallet("", ACCOUNT_PATH, network)
            .map(|(_, created)| created.mnemonic)
    };
    let wallets = Wallets {
        network: network.to_owned(),
        dev: generate()?,
        instance_a: generate()?,
        instance_b: generate()?,
    };
    std::fs::create_dir_all(home()).map_err(|error| error.to_string())?;
    let json = serde_json::to_string_pretty(&wallets).map_err(|error| error.to_string())?;
    std::fs::write(&path, json).map_err(|error| error.to_string())?;
    Ok(wallets)
}
