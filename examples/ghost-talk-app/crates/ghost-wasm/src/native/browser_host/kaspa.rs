use crate::model::{WalletHistoryEntry, WalletProjection, WalletSnapshot};
use ghost_kaspa::{wallet::WalletPublic, PortalFacade};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};

use super::support::util::{required, required_str, to_value};

#[derive(Clone)]
struct BrowserPortal {
    network: String,
    endpoint: String,
    portal: PortalFacade,
}

thread_local! {
    static PORTALS: RefCell<HashMap<String, BrowserPortal>> = RefCell::new(HashMap::new());
}

pub(super) fn endpoint_override<'a>(args: &'a Value, field: &str) -> Option<&'a str> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) async fn invoke_monitor(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "wallet_monitor_start" => monitor_start(args).await,
        "wallet_monitor_snapshot" => monitor_snapshot(args).await,
        "wallet_monitor_update_public" => monitor_update_public(args).await,
        "wallet_monitor_stop" => monitor_stop(args).await,
        _ => Err(format!("unknown browser Kaspa monitor command: {command}")),
    }
}

pub(super) async fn profile_portal(
    profile_id: &str,
    public: &WalletPublic,
    endpoint: Option<&str>,
) -> Result<PortalFacade, String> {
    if let Some(cached) = cached_portal(profile_id, public, endpoint) {
        if cached.current_virtual_daa_score().await.is_ok() {
            return Ok(cached);
        }
        remove_portal(profile_id);
    }
    let portal = connect(public, endpoint).await?;
    portal.current_virtual_daa_score().await.map_err(|error| {
        format!("Kaspa node connected but failed its DAA health check: {error}")
    })?;
    let resolved_endpoint = portal.endpoint()?;
    remember_portal(profile_id, public, resolved_endpoint, portal.clone());
    Ok(portal)
}

async fn monitor_start(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let endpoint = endpoint_override(args, "wrpcEndpoint");
    let portal = profile_portal(profile_id, &public, endpoint).await?;
    super::runtime::live::start(profile_id, &public, &portal).await?;
    Ok(Value::Null)
}

async fn monitor_snapshot(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let projection: WalletProjection = required(args, "public")?;
    let history: Vec<WalletHistoryEntry> = required(args, "history")?;
    let public = WalletPublic::from_projection(&projection);
    let endpoint = endpoint_override(args, "wrpcEndpoint");
    let portal = profile_portal(profile_id, &public, endpoint).await?;
    let addresses = public.all_addresses().cloned().collect::<Vec<_>>();
    let utxos = portal.current_utxos(&addresses).await?;
    let balance = utxos.iter().try_fold(0u64, |total, utxo| {
        total
            .checked_add(utxo.amount)
            .ok_or_else(|| "wallet balance overflow".to_string())
    })?;
    let daa = portal.current_virtual_daa_score().await?;
    let active_addresses = funded_addresses(&addresses, &utxos)?;
    to_value(WalletSnapshot {
        balance_sompi: balance.to_string(),
        utxo_count: utxos.len().to_string(),
        blue_score: daa.to_string(),
        history,
        active_addresses,
        recommended_receive_index: public.next_receive_index,
    })
}

fn funded_addresses(
    addresses: &[String],
    utxos: &[ghost_kaspa::PortalCurrentUtxo],
) -> Result<Vec<String>, String> {
    let funded_scripts = utxos
        .iter()
        .map(|utxo| utxo.script_public_key.as_slice())
        .collect::<Vec<_>>();
    let mut funded = Vec::new();
    for address in addresses {
        let script = PortalFacade::script_pubkey_for_address(address)?;
        if funded_scripts.contains(&script.as_slice()) {
            funded.push(address.clone());
        }
    }
    funded.sort();
    funded.dedup();
    Ok(funded)
}

async fn monitor_update_public(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    if let Some(portal) = cached_portal(profile_id, &public, None) {
        super::runtime::live::start(profile_id, &public, &portal).await?;
    }
    Ok(Value::Null)
}

async fn monitor_stop(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    super::runtime::live::stop(profile_id).await;
    remove_portal(profile_id);
    Ok(Value::Null)
}

fn cached_portal(
    profile_id: &str,
    public: &WalletPublic,
    endpoint: Option<&str>,
) -> Option<PortalFacade> {
    PORTALS.with(|portals| {
        portals.borrow().get(profile_id).and_then(|cached| {
            let endpoint_matches = endpoint.is_none_or(|value| value == cached.endpoint);
            (cached.network == public.network && endpoint_matches).then(|| cached.portal.clone())
        })
    })
}

fn remember_portal(
    profile_id: &str,
    public: &WalletPublic,
    endpoint: String,
    portal: PortalFacade,
) {
    PORTALS.with(|portals| {
        portals.borrow_mut().insert(
            profile_id.to_owned(),
            BrowserPortal {
                network: public.network.clone(),
                endpoint,
                portal,
            },
        );
    });
}

/// Drop the profile's cached connection so the next use reconnects.
pub(super) fn remove_portal(profile_id: &str) {
    PORTALS.with(|portals| {
        portals.borrow_mut().remove(profile_id);
    });
}

async fn connect(public: &WalletPublic, endpoint: Option<&str>) -> Result<PortalFacade, String> {
    match endpoint {
        Some(endpoint) => connect_endpoint(public, endpoint).await,
        None => public_portal(public).await,
    }
}

async fn connect_endpoint(public: &WalletPublic, endpoint: &str) -> Result<PortalFacade, String> {
    validate_wrpc(endpoint)?;
    PortalFacade::connect(&public.network, endpoint).await
}

/// The first healthy public node the resolvers offer, chosen by Kaspa Portal.
async fn public_portal(public: &WalletPublic) -> Result<PortalFacade, String> {
    PortalFacade::connect_public(&public.network)
        .await
        .map_err(|error| {
            format!("No public Kaspa wRPC endpoint was usable in this browser: {error}")
        })
}

fn validate_wrpc(endpoint: &str) -> Result<(), String> {
    if endpoint.starts_with("wss://") || endpoint.starts_with("ws://") {
        Ok(())
    } else {
        Err("Kaspa wRPC endpoint must use ws:// or wss://".into())
    }
}
