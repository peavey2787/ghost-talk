//! In-process p2p-net browser node. Ghost Talk links p2p-net directly and
//! drives its `WasmNode` facade (browser storage, profile lock, lifecycle and
//! WebRTC all stay inside p2p-net).

use ghost_realtime::{ghost_node_config, storage_namespace, P2pStartConfig};
use p2p_net::wasm::WasmNode;
use wasm_bindgen::JsValue;

pub(crate) async fn start_node(config: &P2pStartConfig) -> Result<WasmNode, String> {
    let node_config = ghost_node_config(&config.network_id, &config.infrastructure);
    let value = serde_wasm_bindgen::to_value(&node_config)
        .map_err(|error| format!("p2p-net config: {error}"))?;
    WasmNode::start(value, storage_namespace(&config.profile_id))
        .await
        .map_err(js_error)
}

pub(crate) fn js_error(value: JsValue) -> String {
    js_sys::Reflect::get(&value, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string())
        .or_else(|| value.as_string())
        .unwrap_or_else(|| "p2p-net browser operation failed".into())
}
