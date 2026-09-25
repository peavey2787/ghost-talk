use crate::validation::require_min_password;
use std::{path::PathBuf, sync::Arc};
use zeroize::Zeroize;

use super::super::{
    mailbox_dispatch::profile_path,
    runtime_owner::{release_other_runtimes, HydraRuntimeState},
    runtime_state::HydraProfileRuntime,
    session_types::{AsyncMutex, HydraFacade, HydraReady, State, WalletPublic},
    transport_persistence::transport_state_key_from_seed,
};

/// Materialize the one Ghost Talk HYDRA identity deterministically from the
/// profile's 24-word Kaspa BIP39 root. There is intentionally no independent
/// HYDRA mnemonic/import path: the Kaspa mnemonic + optional BIP39 passphrase
/// is the complete cryptographic recovery root for this development line.
fn initialize_from_wallet_blocking(
    open_path: PathBuf,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<(HydraReady, HydraFacade, [u8; 32]), String> {
    require_min_password(&password, "ID")?;
    let secret = crate::wallet_commands::open_secret(&password, &sealed)?;
    crate::wallet_commands::validate_public_projection(&secret, &public)?;
    let mut seed = ghost_kaspa::wallet::hydra_identity_seed(&secret)?;
    let transport_state_key = transport_state_key_from_seed(&seed);
    let mut hydra = HydraFacade::open(open_path, &password)?;
    let identity_id = ensure_wallet_identity(&mut hydra, &password, &mut seed)?;
    let identity = hydra
        .list_identities()
        .into_iter()
        .find(|identity| identity.id == identity_id)
        .ok_or_else(|| "HYDRA identity disappeared after initialization".to_string())?;
    Ok((
        HydraReady {
            identity_id: identity.id,
            label: identity.label,
        },
        hydra,
        transport_state_key,
    ))
}

fn ensure_wallet_identity(
    hydra: &mut HydraFacade,
    password: &str,
    seed: &mut [u8; 32],
) -> Result<String, String> {
    let identities = hydra.list_identities();
    match identities.as_slice() {
        [] => {
            let result = hydra.import_identity_seed(*seed, password);
            seed.zeroize();
            result
        }
        [existing] => resume_wallet_identity(hydra, password, seed, existing),
        _ => {
            seed.zeroize();
            Err("this Ghost Talk profile contains multiple HYDRA identities; current profiles support exactly one wallet-derived identity".into())
        }
    }
}

fn resume_wallet_identity(
    hydra: &mut HydraFacade,
    password: &str,
    seed: &mut [u8; 32],
    existing: &ghost_hydra::IdentityProjection,
) -> Result<String, String> {
    let mut stored_seed = hydra.export_identity_seed(&existing.id, password)?;
    let matches = stored_seed == *seed;
    stored_seed.zeroize();
    seed.zeroize();
    if !matches {
        return Err("this profile contains a HYDRA identity not derived from its Ghost Talk recovery root; create or restore a current account instead".into());
    }
    hydra.set_active_identity(&existing.id, password)?;
    Ok(existing.id.clone())
}

#[tauri::command]
pub async fn hydra_initialize_from_wallet(
    app: State<'_, crate::NativeAppState>,
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<HydraReady, String> {
    let app = app.handle();
    let _lifecycle = state.lifecycle.lock().await;
    if let Some(existing) = state.runtime_if_present(&profile_id)? {
        return ready_from_runtime(existing, None, &password).await;
    }
    release_other_runtimes(&state, &profile_id).await?;
    let runtime_profile_id = profile_id.clone();
    let hydra_profile_path = profile_path(&app, &profile_id)?;
    let open_path = hydra_profile_path.clone();
    let (ready, hydra, mut transport_state_key) =
        crate::run_blocking("HYDRA identity initialization", move || {
            initialize_from_wallet_blocking(open_path, password, sealed, public)
        })
        .await?;
    let install_result = state.install(
        runtime_profile_id,
        ready.identity_id.clone(),
        hydra,
        hydra_profile_path,
        transport_state_key,
    );
    transport_state_key.zeroize();
    install_result?;
    Ok(ready)
}

pub(crate) async fn ready_from_runtime(
    runtime: Arc<AsyncMutex<HydraProfileRuntime>>,
    identity_id: Option<&str>,
    password: &str,
) -> Result<HydraReady, String> {
    let mut runtime = runtime.lock().await;
    let selected = identity_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(runtime.identity_id.as_str())
        .to_owned();
    if selected != runtime.identity_id {
        return Err("selected HYDRA identity is not present in this profile".into());
    }

    // Reuse the currently managed handle for repeated unlocks of the same ID.
    // Switching IDs removes and drops this runtime through hydra_lock_profile or
    // release_other_runtimes before another native profile is opened. An identity
    // reported unlocked on this managed facade was unlocked by this facade, so its
    // application signing key is already resident too. Do not run HYDRA's scrypt
    // identity-unlock/export path again just to return the same ready projection.
    let mut identity = runtime
        .hydra
        .list_identities()
        .into_iter()
        .find(|identity| identity.id == selected)
        .ok_or_else(|| "HYDRA identity disappeared from the open profile".to_string())?;
    if !identity.unlocked {
        runtime.hydra.set_active_identity(&selected, password)?;
        identity = runtime
            .hydra
            .list_identities()
            .into_iter()
            .find(|identity| identity.id == selected)
            .ok_or_else(|| "HYDRA identity disappeared after unlock".to_string())?;
    }
    Ok(HydraReady {
        identity_id: identity.id,
        label: identity.label,
    })
}

pub(crate) async fn open_hydra_profile(
    path: PathBuf,
    password: String,
    identity_id: Option<String>,
    identity_seed: [u8; 32],
) -> Result<(HydraReady, HydraFacade), String> {
    crate::run_blocking("HYDRA identity unlock", move || {
        let mut identity_seed = identity_seed;
        let result = (|| {
            let mut hydra = HydraFacade::open(path, &password)?;
            let identities = hydra.list_identities();
            if identities.len() != 1 {
                return Err(
                    "current Ghost Talk profiles require exactly one wallet-derived HYDRA identity"
                        .into(),
                );
            }
            let selected = select_hydra_identity(&identities, identity_id)?;
            hydra.set_active_identity_with_seed(&selected, &password, identity_seed)?;
            let identity = hydra
                .list_identities()
                .into_iter()
                .find(|identity| identity.id == selected)
                .ok_or_else(|| "HYDRA identity disappeared after unlock".to_string())?;
            Ok((
                HydraReady {
                    identity_id: identity.id,
                    label: identity.label,
                },
                hydra,
            ))
        })();
        identity_seed.zeroize();
        result
    })
    .await
}

pub(crate) fn select_hydra_identity(
    identities: &[ghost_hydra::IdentityProjection],
    requested: Option<String>,
) -> Result<String, String> {
    let Some(requested) = requested.filter(|value| !value.trim().is_empty()) else {
        return Ok(identities[0].id.clone());
    };
    if identities[0].id == requested {
        Ok(requested)
    } else {
        Err("selected HYDRA identity is not present in this profile".into())
    }
}

pub(crate) async fn existing_ready(
    state: &HydraRuntimeState,
    profile_id: &str,
    identity_id: Option<&str>,
    password: &str,
) -> Result<Option<HydraReady>, String> {
    let Some(existing) = state.runtime_if_present(profile_id)? else {
        return Ok(None);
    };
    ready_from_runtime(existing, identity_id, password)
        .await
        .map(Some)
}
