use super::restore::peer_session_binding;
pub(crate) use crate::model::{
    BackupContact, BackupMessage, BroadcastResult, Chat, DebugLogSnapshot, HydraControlProjection,
    HydraMailboxResult, HydraRealtimeEnvelope, HydraReady, HydraRecoveryProjection,
    HydraSessionBindingProjection, MailboxSendResult, PeerRouteRegistration, Profile,
    ProfileBackupPublishResult, ProfileBackupRestoreResult, PublicGhostProfile,
    PublishedGhostDescriptor, ResolvedGhostPeer, WalletCreateResponse, WalletImportResponse,
    WalletProjection, WalletRecord, WalletRecovery, WalletSnapshot,
};
pub(crate) use js_sys::{Function, Promise, Reflect};
pub(crate) use serde::{de::DeserializeOwned, Serialize};
pub(crate) use serde_json::{json, Value};
pub(crate) use wasm_bindgen::{closure::Closure, JsCast, JsValue};
pub(crate) use wasm_bindgen_futures::JsFuture;

pub use ghost_core::SESSION_RESTORE_BODY;

pub(crate) fn property(target: &JsValue, name: &str) -> Result<JsValue, String> {
    Reflect::get(target, &JsValue::from_str(name))
        .map_err(|_| format!("Tauri API property lookup failed for {name}"))
}

pub(crate) fn tauri_root() -> Result<JsValue, String> {
    let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
    let window_value: JsValue = window.into();
    for name in ["__TAURI__", "__TAURI_INTERNALS__"] {
        let value = property(&window_value, name)?;
        if !value.is_undefined() && !value.is_null() {
            return Ok(value);
        }
    }
    Err("Tauri API is unavailable in this browser context".to_string())
}

pub(crate) fn tauri_core() -> Result<JsValue, String> {
    let root = tauri_root()?;
    let core = property(&root, "core")?;
    if core.is_undefined() || core.is_null() {
        // Tauri internals exposes invoke directly instead of the public global modules.
        return Ok(root);
    }
    Ok(core)
}

pub fn is_tauri() -> bool {
    tauri_root().is_ok()
}

pub(crate) async fn invoke_js(command: &str, args: &JsValue) -> Result<JsValue, String> {
    let core = tauri_core()?;
    let function = property(&core, "invoke")?
        .dyn_into::<Function>()
        .map_err(|_| "Tauri invoke is not callable".to_string())?;
    let value = function
        .call2(&core, &JsValue::from_str(command), args)
        .map_err(js_error)?;
    let promise = value
        .dyn_into::<Promise>()
        .map_err(|_| "Tauri invoke did not return a Promise".to_string())?;
    JsFuture::from(promise).await.map_err(js_error)
}

pub async fn invoke<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, String> {
    if !is_tauri() {
        let value = super::browser_host::invoke(command, args).await?;
        return serde_json::from_value(value)
            .map_err(|error| format!("{command} response: {error}"));
    }
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let js_args = args
        .serialize(&serializer)
        .map_err(|error| error.to_string())?;
    let value = invoke_js(command, &js_args).await?;
    serde_wasm_bindgen::from_value(value).map_err(|error| format!("{command} response: {error}"))
}

pub async fn invoke_unit(command: &str, args: Value) -> Result<(), String> {
    if !is_tauri() {
        super::browser_host::invoke(command, args).await?;
        return Ok(());
    }
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let js_args = args
        .serialize(&serializer)
        .map_err(|error| error.to_string())?;
    invoke_js(command, &js_args).await.map(|_| ())
}

pub fn js_error(value: JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }
    js_sys::JSON::stringify(&value)
        .ok()
        .and_then(|text| text.as_string())
        .unwrap_or_else(|| "JavaScript bridge error".to_string())
}

pub async fn load_profile_state() -> Result<Option<String>, String> {
    invoke("profile_state_load", json!({})).await
}

pub async fn save_profile_state(value: &str, changed_profile_ids: &[String]) -> Result<(), String> {
    invoke_unit(
        "profile_state_save",
        json!({
            "json": value,
            "changedProfileIds": changed_profile_ids,
        }),
    )
    .await
}

pub async fn create_wallet(
    password: &str,
    passphrase: &str,
    account_path: &str,
    network: &str,
) -> Result<WalletCreateResponse, String> {
    invoke(
        "wallet_create",
        json!({
            "password": password,
            "passphrase": passphrase,
            "accountPath": account_path,
            "network": network,
        }),
    )
    .await
}

pub async fn import_wallet(
    password: &str,
    mnemonic: &str,
    passphrase: &str,
    account_path: &str,
    network: &str,
) -> Result<WalletImportResponse, String> {
    invoke(
        "wallet_import",
        json!({
            "password": password,
            "mnemonic": mnemonic,
            "passphrase": passphrase,
            "accountPath": account_path,
            "network": network,
        }),
    )
    .await
}

pub async fn initialize_hydra(
    profile_id: &str,
    password: &str,
    wallet: &WalletRecord,
) -> Result<HydraReady, String> {
    invoke(
        "hydra_initialize_from_wallet",
        json!({
            "profileId": profile_id,
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
        }),
    )
    .await
}

pub async fn ensure_hydra(
    profile_id: &str,
    password: &str,
    identity_id: Option<&str>,
) -> Result<HydraReady, String> {
    invoke(
        "hydra_ensure",
        json!({
            "profileId": profile_id,
            "password": password,
            "identityId": identity_id,
        }),
    )
    .await
}

mod routes;
pub(crate) use routes::{
    canonical_peer_routes, inspect_surviving_bindings, migrate_session_roles, register_peer_routes,
};
