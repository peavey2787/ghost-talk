use super::invoke::{
    invoke, invoke_unit, is_tauri, json, property, tauri_root, Closure, DebugLogSnapshot,
    DeserializeOwned, Function, JsFuture, JsValue, Profile, Promise, Reflect, Value,
    WalletProjection, WalletSnapshot,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use wasm_bindgen::JsCast;

type BrowserHandler = Rc<RefCell<dyn FnMut(Value)>>;

thread_local! {
    static BROWSER_LISTENERS: RefCell<HashMap<&'static str, Vec<BrowserHandler>>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn emit_browser(event_name: &'static str, payload: Value) {
    BROWSER_LISTENERS.with(|listeners| {
        let handlers = listeners
            .borrow()
            .get(event_name)
            .cloned()
            .unwrap_or_default();
        for handler in handlers {
            (handler.borrow_mut())(payload.clone());
        }
    });
}

fn listen_browser<T, F>(event_name: &'static str, mut handler: F)
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    let callback: BrowserHandler = Rc::new(RefCell::new(move |payload: Value| {
        if let Ok(value) = serde_json::from_value::<T>(payload) {
            handler(value);
        }
    }));
    BROWSER_LISTENERS.with(|listeners| {
        listeners
            .borrow_mut()
            .entry(event_name)
            .or_default()
            .push(callback);
    });
}

pub async fn start_wallet_monitor(profile: &Profile) -> Result<(), String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke_unit(
        "wallet_monitor_start",
        json!({
            "profileId": profile.id,
            "public": wallet.public,
            "checkpoint": wallet.mailbox_checkpoint,
            "history": wallet.history,
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn browser_wallet_snapshot(profile: &Profile) -> Result<WalletSnapshot, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "wallet_monitor_snapshot",
        json!({
            "profileId": profile.id,
            "public": wallet.public,
            "history": wallet.history,
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn update_wallet_monitor_public(
    profile_id: &str,
    public: &WalletProjection,
) -> Result<(), String> {
    invoke_unit(
        "wallet_monitor_update_public",
        json!({ "profileId": profile_id, "public": public }),
    )
    .await
}

pub async fn set_remembered_unlock(
    profile: &Profile,
    enabled: bool,
    password: &str,
) -> Result<(), String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke_unit(
        "remembered_unlock_set",
        json!({
            "profileId": profile.id,
            "enabled": enabled,
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
        }),
    )
    .await
}

pub async fn load_remembered_unlock(profile_id: &str) -> Result<Option<String>, String> {
    invoke("remembered_unlock_load", json!({ "profileId": profile_id })).await
}

pub async fn set_debug_logging(enabled: bool) -> Result<(), String> {
    invoke_unit("debug_log_set_enabled", json!({ "enabled": enabled })).await
}

pub async fn debug_log_snapshot(since_sequence: Option<u64>) -> Result<DebugLogSnapshot, String> {
    invoke(
        "debug_log_snapshot",
        json!({ "sinceSequence": since_sequence }),
    )
    .await
}

pub async fn clear_debug_log() -> Result<(), String> {
    invoke_unit("debug_log_clear", json!({})).await
}

pub async fn record_debug(category: &str, event: &str, details: &str) -> Result<(), String> {
    invoke_unit(
        "debug_log_record",
        json!({ "level": "info", "category": category, "event": event, "details": details }),
    )
    .await
}

pub async fn hydra_debug_state(profile_id: &str) -> Result<Value, String> {
    invoke("hydra_debug_state", json!({ "profileId": profile_id })).await
}

pub fn listen<T, F>(event_name: &'static str, handler: F)
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    if !is_tauri() {
        listen_browser::<T, F>(event_name, handler);
        return;
    }
    listen_tauri::<T, F>(event_name, handler);
}

fn listen_tauri<T, F>(event_name: &'static str, mut handler: F)
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(root) = tauri_root() else { return };
        let Ok(event_api) = property(&root, "event") else {
            return;
        };
        if event_api.is_undefined() || event_api.is_null() {
            return;
        }
        let Ok(listen) = property(&event_api, "listen").and_then(|value| {
            value
                .dyn_into::<Function>()
                .map_err(|_| "Tauri event.listen is not callable".to_string())
        }) else {
            return;
        };
        let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            let payload =
                Reflect::get(&event, &JsValue::from_str("payload")).unwrap_or(JsValue::UNDEFINED);
            if let Ok(value) = serde_wasm_bindgen::from_value::<T>(payload) {
                handler(value);
            }
        });
        let Ok(result) = listen.call2(
            &event_api,
            &JsValue::from_str(event_name),
            closure.as_ref().unchecked_ref(),
        ) else {
            return;
        };
        let Ok(promise) = result.dyn_into::<Promise>() else {
            return;
        };
        if JsFuture::from(promise).await.is_ok() {
            closure.forget();
        }
    });
}
