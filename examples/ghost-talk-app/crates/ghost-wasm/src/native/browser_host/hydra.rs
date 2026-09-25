use crate::model::{HydraReady, WalletProjection};
use ghost_hydra::HydraFacade;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use serde_json::Value;
use zeroize::Zeroize;

use super::{
    support::util::{optional_str, require_password, required, required_str, to_value},
    HYDRA_RUNTIMES, WALLET_SECRETS,
};

pub(super) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    if let Some(result) = invoke_contact(command, args) {
        return result;
    }
    if let Some(result) = invoke_transport(command, args) {
        return result;
    }
    lifecycle_invoke(command, args).await
}


fn invoke_contact(command: &str, args: &Value) -> Option<Result<Value, String>> {
    match command {
        "hydra_preview_contact_request" => Some(preview_contact_request(args)),
        "hydra_register_peer_routes" => Some(super::runtime::secure_transport::register_routes(args)),
        "hydra_peer_session_binding" => Some(super::runtime::secure_transport::binding(args)),
        _ => None,
    }
}

fn invoke_transport(command: &str, args: &Value) -> Option<Result<Value, String>> {
    match command {
        "hydra_seal_realtime" => Some(super::runtime::realtime::seal(args)),
        "hydra_open_realtime" => Some(super::runtime::realtime::open(args)),
        "hydra_leave_peer" => Some(super::runtime::secure_transport::leave(args)),
        "hydra_rejoin_peer" => Some(super::runtime::secure_transport::rejoin(args)),
        _ => None,
    }
}

fn preview_contact_request(args: &Value) -> Result<Value, String> {
    let envelope_hex = required_str(args, "envelopeHex")?;
    let local_addresses: Vec<String> = required(args, "localKaspaAddresses")?;
    to_value(ghost_kaspa::preview_contact_request(envelope_hex, &local_addresses)?)
}

async fn lifecycle_invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "hydra_debug_state" => debug_state(args),
        "hydra_initialize_from_wallet" => initialize_from_wallet(args).await,
        "hydra_ensure" => ensure(args).await,
        "hydra_lock_profile" => lock_profile(args).await,
        other => Err(format!("unknown browser HYDRA command: {other}")),
    }
}

fn debug_state(args: &Value) -> Result<Value, String> {
    if !super::support::debug::enabled() {
        return Ok(serde_json::json!({"available": false, "sessions": []}));
    }
    let profile_id = required_str(args, "profileId")?;
    super::runtime::session::debug_value(profile_id).or_else(|_| {
        Ok(serde_json::json!({"available": false, "sessions": []}))
    })
}

async fn initialize_from_wallet(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?.to_owned();
    let password = required_str(args, "password")?.to_owned();
    require_password(&password, "ID")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let secret: WalletSecret = ghost_storage::open_json(&password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    let mut seed = ghost_kaspa::wallet::hydra_identity_seed(&secret)?;
    let mut hydra = HydraFacade::open_browser(&store_name(&profile_id), &password).await?;
    let ready_result = ensure_wallet_identity(&mut hydra, &password, &mut seed);
    seed.zeroize();
    let ready = ready_result?;
    hydra.flush_browser().await?;
    WALLET_SECRETS.with(|secrets| {
        secrets.borrow_mut().insert(profile_id.clone(), secret);
    });
    super::runtime::session::set_identity(&profile_id, &ready.identity_id);
    HYDRA_RUNTIMES.with(|runtimes| {
        runtimes.borrow_mut().insert(profile_id, hydra);
    });
    to_value(ready)
}

async fn ensure(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?.to_owned();
    let password = required_str(args, "password")?.to_owned();
    require_password(&password, "ID")?;
    let requested = optional_str(args, "identityId").map(str::to_owned);

    if let Some(mut hydra) = take_runtime(&profile_id) {
        let ready = ready_existing(&mut hydra, requested.as_deref(), &password)?;
        super::runtime::session::set_identity(&profile_id, &ready.identity_id);
        put_runtime(profile_id, hydra);
        return to_value(ready);
    }

    let secret = WALLET_SECRETS.with(|secrets| secrets.borrow().get(&profile_id).cloned())
        .ok_or_else(|| "wallet must be unlocked before HYDRA can open".to_string())?;
    let seed = ghost_kaspa::wallet::hydra_identity_seed(&secret)?;
    let mut hydra = HydraFacade::open_browser(&store_name(&profile_id), &password).await?;
    let identities = hydra.list_identities();
    if identities.len() != 1 {
        return Err(
            "current Ghost Talk profiles require exactly one wallet-derived HYDRA identity".into(),
        );
    }
    let selected = requested
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| identities[0].id.clone());
    if selected != identities[0].id {
        return Err("selected HYDRA identity is not present in this profile".into());
    }
    hydra.set_active_identity_with_seed(&selected, &password, seed)?;
    let ready = ready_existing(&mut hydra, Some(&selected), &password)?;
    hydra.flush_browser().await?;
    super::runtime::session::set_identity(&profile_id, &ready.identity_id);
    put_runtime(profile_id, hydra);
    to_value(ready)
}

async fn lock_profile(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?.to_owned();
    if let Some(mut hydra) = take_runtime(&profile_id) {
        hydra.lock_active_identity()?;
        hydra.flush_browser().await?;
    }
    super::runtime::session::clear(&profile_id);
    Ok(Value::Null)
}

fn ensure_wallet_identity(
    hydra: &mut HydraFacade,
    password: &str,
    seed: &mut [u8; 32],
) -> Result<HydraReady, String> {
    let identities = hydra.list_identities();
    let identity_id = match identities.as_slice() {
        [] => hydra.import_identity_seed(*seed, password)?,
        [existing] => resume_wallet_identity(hydra, existing, password, seed)?,
        _ => {
            return Err("this Ghost Talk profile contains multiple HYDRA identities; current profiles support exactly one wallet-derived identity".into())
        }
    };
    ready_by_id(hydra, &identity_id, "initialization")
}

fn resume_wallet_identity(
    hydra: &mut HydraFacade,
    existing: &ghost_hydra::IdentityProjection,
    password: &str,
    seed: &[u8; 32],
) -> Result<String, String> {
    let mut stored_seed = hydra.export_identity_seed(&existing.id, password)?;
    let matches = stored_seed == *seed;
    stored_seed.zeroize();
    if !matches {
        return Err("this profile contains a HYDRA identity not derived from its Ghost Talk recovery root; create or restore a current account instead".into());
    }
    hydra.set_active_identity_with_seed(&existing.id, password, *seed)?;
    Ok(existing.id.clone())
}

fn ready_existing(
    hydra: &mut HydraFacade,
    requested: Option<&str>,
    password: &str,
) -> Result<HydraReady, String> {
    let identities = hydra.list_identities();
    if identities.len() != 1 {
        return Err(
            "current Ghost Talk profiles require exactly one wallet-derived HYDRA identity".into(),
        );
    }
    let selected = requested
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(identities[0].id.as_str());
    if selected != identities[0].id {
        return Err("selected HYDRA identity is not present in this profile".into());
    }
    if !identities[0].unlocked {
        hydra.set_active_identity(selected, password)?;
    }
    ready_by_id(hydra, selected, "unlock")
}

fn ready_by_id(hydra: &HydraFacade, identity_id: &str, action: &str) -> Result<HydraReady, String> {
    let identity = hydra
        .list_identities()
        .into_iter()
        .find(|identity| identity.id == identity_id)
        .ok_or_else(|| format!("HYDRA identity disappeared after {action}"))?;
    Ok(HydraReady {
        identity_id: identity.id,
        label: identity.label,
    })
}

fn take_runtime(profile_id: &str) -> Option<HydraFacade> {
    HYDRA_RUNTIMES.with(|runtimes| runtimes.borrow_mut().remove(profile_id))
}

fn put_runtime(profile_id: String, hydra: HydraFacade) {
    HYDRA_RUNTIMES.with(|runtimes| {
        runtimes.borrow_mut().insert(profile_id, hydra);
    });
}

fn store_name(profile_id: &str) -> String {
    format!("ghost-talk-hydra-{profile_id}")
}
