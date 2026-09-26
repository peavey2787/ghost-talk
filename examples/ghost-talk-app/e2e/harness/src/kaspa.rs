//! Dev-wallet balance checks and instance funding through the application's
//! own Kaspa Portal wallet code.

use std::time::Duration;

use ghost_kaspa::{
    wallet::{self, WalletPublic, WalletSecret},
    PortalFacade,
};
use serde_json::json;

use crate::state::{self, ACCOUNT_PATH};

const SOMPI_PER_KAS: f64 = 100_000_000.0;

struct Account {
    secret: WalletSecret,
    public: WalletPublic,
}

impl Account {
    fn new(mnemonic: &str, network: &str) -> Result<Self, String> {
        let (secret, public) = wallet::import_wallet(mnemonic, "", ACCOUNT_PATH, network)?;
        Ok(Self { secret, public })
    }

    fn address(&self) -> Result<String, String> {
        self.public
            .receive_addresses
            .first()
            .cloned()
            .ok_or_else(|| "wallet has no receive address".into())
    }

    async fn utxo_count(&self, portal: &PortalFacade) -> Result<usize, String> {
        let addresses: Vec<String> = self.public.all_addresses().cloned().collect();
        Ok(portal.current_utxos(&addresses).await?.len())
    }

    async fn balance(&self, portal: &PortalFacade) -> Result<u64, String> {
        let addresses: Vec<String> = self.public.all_addresses().cloned().collect();
        let utxos = portal.current_utxos(&addresses).await?;
        Ok(utxos.iter().map(|utxo| utxo.amount).sum())
    }
}

/// Print wallet addresses and live balances as one JSON object.
pub async fn status(network: &str) -> Result<(), String> {
    let wallets = state::load_or_create(network)?;
    let portal = connect(network).await?;
    let mut entries = serde_json::Map::new();
    for (name, mnemonic) in [
        ("dev", &wallets.dev),
        ("instanceA", &wallets.instance_a),
        ("instanceB", &wallets.instance_b),
    ] {
        let account = Account::new(mnemonic, network)?;
        let balance = account.balance(&portal).await?;
        let mut entry =
            json!({ "address": account.address()?, "balanceKas": balance as f64 / SOMPI_PER_KAS });
        if name != "dev" {
            entry["mnemonic"] = json!(mnemonic);
        }
        entries.insert(name.into(), entry);
    }
    let home = state::home().display().to_string();
    println!(
        "{}",
        json!({ "network": network, "home": home, "wallets": entries })
    );
    Ok(())
}

pub const FAUCET_URL: &str = "https://faucet-tn10.kaspanet.io/";

/// Block until the dev wallet holds at least `min` sompi. When it does not,
/// show its address and the faucet, and re-check after the user types `y`.
pub async fn ensure_funded(network: &str, min: u64) -> Result<(), String> {
    let wallets = state::load_or_create(network)?;
    let dev = Account::new(&wallets.dev, network)?;
    let address = dev.address()?;
    loop {
        let portal = connect(network).await?;
        let balance = dev.balance(&portal).await?;
        if balance >= min {
            eprintln!(
                "Dev wallet {address} holds {} KAS.",
                balance as f64 / SOMPI_PER_KAS
            );
            return Ok(());
        }
        eprintln!();
        eprintln!(
            "The E2E dev wallet needs at least {} test KAS ({network}).",
            min as f64 / SOMPI_PER_KAS
        );
        eprintln!("  Current balance: {} KAS", balance as f64 / SOMPI_PER_KAS);
        eprintln!("  Fund this address: {address}");
        eprintln!("  Testnet-10 faucet: {FAUCET_URL}");
        eprintln!("  Wallet file:       {}", state::home().display());
        if !confirmed()? {
            return Err("dev wallet is not funded; E2E run cancelled".into());
        }
    }
}

fn confirmed() -> Result<bool, String> {
    loop {
        eprint!("Type 'y' when the wallet is funded and ready to continue (or 'n' to cancel): ");
        let mut line = String::new();
        let read = std::io::stdin()
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        match line.trim().to_ascii_lowercase().as_str() {
            _ if read == 0 => return Ok(false),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => {}
        }
    }
}

/// Top up each instance wallet from the dev wallet when it is below `min`.
pub async fn fund(network: &str, min: u64, topup: u64) -> Result<(), String> {
    let wallets = state::load_or_create(network)?;
    let portal = connect(network).await?;
    let dev = Account::new(&wallets.dev, network)?;
    for mnemonic in [&wallets.instance_a, &wallets.instance_b] {
        let instance = Account::new(mnemonic, network)?;
        consolidate_if_fragmented(&portal, &instance).await?;
        let balance = instance.balance(&portal).await?;
        if balance >= min {
            continue;
        }
        let address = instance.address()?;
        let sent = wallet::send(&portal, &dev.secret, &dev.public, &address, topup, 0).await?;
        eprintln!(
            "funded {address} with {} KAS (tx {})",
            topup as f64 / SOMPI_PER_KAS,
            sent.transaction_id
        );
        wait_for_balance(&portal, &instance, min).await?;
    }
    Ok(())
}

/// Repeated E2E runs leave many small mailbox outputs behind; merge them so
/// each run starts from a wallet the planner can spend cheaply.
async fn consolidate_if_fragmented(portal: &PortalFacade, account: &Account) -> Result<(), String> {
    const MAX_UTXOS: usize = 16;
    if account.utxo_count(portal).await? <= MAX_UTXOS {
        return Ok(());
    }
    let merged = wallet::consolidate(portal, &account.secret, &account.public, 0).await?;
    eprintln!(
        "consolidated {} (tx {})",
        account.address()?,
        merged.transaction_id
    );
    for _ in 0..60 {
        if account.utxo_count(portal).await? <= MAX_UTXOS {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}

async fn wait_for_balance(
    portal: &PortalFacade,
    account: &Account,
    min: u64,
) -> Result<(), String> {
    for _ in 0..120 {
        if account.balance(portal).await? >= min {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("timed out waiting for the funding transaction to appear".into())
}

/// Connect to a public Kaspa node through the public resolvers.
async fn connect(network: &str) -> Result<PortalFacade, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| error.to_string())?;
    let mut errors = Vec::new();
    for resolver in ghost_kaspa::PUBLIC_WRPC_RESOLVERS {
        match resolve(&client, resolver, network).await {
            Ok(endpoint) => match PortalFacade::connect(network, &endpoint).await {
                Ok(portal) => return Ok(portal),
                Err(error) => errors.push(format!("{endpoint}: {error}")),
            },
            Err(error) => errors.push(format!("{resolver}: {error}")),
        }
    }
    Err(format!(
        "no public Kaspa node reachable: {}",
        errors.join("; ")
    ))
}

async fn resolve(
    client: &reqwest::Client,
    resolver: &str,
    network: &str,
) -> Result<String, String> {
    let url = ghost_kaspa::resolver_query_url(resolver, network)?;
    let value: serde_json::Value = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    value["url"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "resolver returned no url".into())
}
