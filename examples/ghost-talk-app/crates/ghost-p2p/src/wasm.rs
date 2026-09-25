use std::sync::{Arc, RwLock};

use js_sys::{Reflect, Uint8Array};
use p2p_net::{NodeConfig, NodeEvent};
use wasm_bindgen::JsValue;

use crate::{
    GhostP2pTransport, LocalP2pBinding, P2pFuture, P2pIncoming, P2pStartConfig,
    P2pSubscription, SessionPeerBinding, SessionPeerBindings,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum P2pNetEvent {
    LocalBindingChanged(LocalP2pBinding),
    PeerConnected(String),
    PeerDisconnected(String),
    Online,
    Offline,
}

pub struct P2pNetTransport {
    node: Option<p2p_net::wasm::WasmNode>,
    bindings: Arc<RwLock<SessionPeerBindings>>,
}

impl Default for P2pNetTransport {
    fn default() -> Self {
        Self {
            node: None,
            bindings: Arc::new(RwLock::new(SessionPeerBindings::default())),
        }
    }
}

pub struct P2pNetSubscription {
    inner: p2p_net::wasm::WasmSubscription,
    bindings: Arc<RwLock<SessionPeerBindings>>,
    sid: [u8; 16],
}

pub struct P2pNetEventSubscription {
    inner: p2p_net::wasm::WasmEventSubscription,
}

impl P2pNetTransport {
    pub fn subscribe_events(&self) -> Result<P2pNetEventSubscription, String> {
        let node = self
            .node
            .as_ref()
            .ok_or_else(|| "p2p-net has not started".to_string())?;
        Ok(P2pNetEventSubscription {
            inner: node.subscribe_events(),
        })
    }

    pub fn has_bound_peer(&self, peer_id: &str) -> bool {
        self.bindings
            .read()
            .map(|bindings| bindings.contains_peer(peer_id))
            .unwrap_or(false)
    }
}

impl P2pNetEventSubscription {
    pub async fn next(&mut self) -> Result<P2pNetEvent, String> {
        let value = self.inner.recv().await.map_err(js_error)?;
        let event: NodeEvent = serde_wasm_bindgen::from_value(value)
            .map_err(|error| format!("invalid p2p-net node event: {error}"))?;
        Ok(match event {
            NodeEvent::LocalBindingChanged(binding) => {
                P2pNetEvent::LocalBindingChanged(LocalP2pBinding {
                    peer_id: binding.peer_id,
                    dial_addresses: binding.dial_addresses,
                })
            }
            NodeEvent::PeerConnected { peer_id } => P2pNetEvent::PeerConnected(peer_id),
            NodeEvent::PeerDisconnected { peer_id } => P2pNetEvent::PeerDisconnected(peer_id),
            NodeEvent::Online => P2pNetEvent::Online,
            NodeEvent::Offline => P2pNetEvent::Offline,
        })
    }
}

impl P2pSubscription for P2pNetSubscription {
    fn next<'a>(&'a mut self) -> P2pFuture<'a, Option<P2pIncoming>> {
        Box::pin(async move {
            loop {
                let value = match self.inner.recv().await {
                    Ok(value) => value,
                    Err(error) => return Err(js_error(error)),
                };
                let source_peer_id = string_field(&value, "sourcePeerId")?;
                let topic = string_field(&value, "topic")?;
                let payload_value = Reflect::get(&value, &JsValue::from_str("payload"))
                    .map_err(js_error)?;
                let packet = Uint8Array::new(&payload_value).to_vec();
                let carrier = match ghost_protocol::Gtr1Envelope::decode(&packet) {
                    Ok(carrier) => carrier,
                    Err(_) => continue,
                };
                if carrier.sid != self.sid || topic != ghost_protocol::session_topic(&self.sid) {
                    continue;
                }
                let valid_source = self
                    .bindings
                    .read()
                    .map_err(|_| "p2p session bindings are poisoned".to_string())?
                    .verify_source(&self.sid, &carrier.sender_hydra_id, &source_peer_id);
                if !valid_source {
                    continue;
                }
                return Ok(Some(P2pIncoming {
                    source_peer_id,
                    topic,
                    packet,
                }));
            }
        })
    }
}

impl GhostP2pTransport for P2pNetTransport {
    type Subscription = P2pNetSubscription;

    fn start<'a>(&'a mut self, config: P2pStartConfig) -> P2pFuture<'a, LocalP2pBinding> {
        Box::pin(async move {
            if self.node.is_some() {
                return self.local_binding().await;
            }
            let mut cfg = NodeConfig::default();
            cfg.network_id = network_id(&config.network_id);
            cfg.identity_key_path = "identity.key".into();
            cfg.discovery.peer_cache_path = "peer-cache.json".into();
            let js_config = serde_wasm_bindgen::to_value(&cfg)
                .map_err(|error| format!("could not serialize p2p-net config: {error}"))?;
            // Persistence is profile-scoped so changing Kaspa networks does not
            // silently rotate the Ghost p2p identity. The libp2p network
            // discriminator remains network-specific in NodeConfig above.
            let namespace = format!("ghost-talk:{}", config.profile_id);
            let node = p2p_net::wasm::WasmNode::start(js_config, namespace)
                .await
                .map_err(js_error)?;
            self.node = Some(node);
            self.local_binding().await
        })
    }

    fn local_binding<'a>(&'a self) -> P2pFuture<'a, LocalP2pBinding> {
        Box::pin(async move {
            let node = self
                .node
                .as_ref()
                .ok_or_else(|| "p2p-net has not started".to_string())?;
            let value = node.local_binding().await.map_err(js_error)?;
            let binding: p2p_net::LocalNodeBinding = serde_wasm_bindgen::from_value(value)
                .map_err(|error| format!("invalid p2p-net local binding: {error}"))?;
            let peer_id = binding.peer_id;
            let dial_addresses = binding
                .dial_addresses
                .into_iter()
                .map(|address| ensure_peer_suffix(address, &peer_id))
                .collect();
            Ok(LocalP2pBinding {
                peer_id,
                dial_addresses,
            })
        })
    }

    fn bind_session<'a>(&'a mut self, binding: SessionPeerBinding) -> P2pFuture<'a, ()> {
        Box::pin(async move {
            self.bindings
                .write()
                .map_err(|_| "p2p session bindings are poisoned".to_string())?
                .bind(binding)
        })
    }

    fn connect<'a>(&'a mut self, address: &'a str) -> P2pFuture<'a, ()> {
        Box::pin(async move {
            let node = self
                .node
                .as_ref()
                .ok_or_else(|| "p2p-net has not started".to_string())?;
            node.connect_peer(address.to_string()).await.map_err(js_error)
        })
    }

    fn send<'a>(&'a self, sid: [u8; 16], packet: &'a [u8]) -> P2pFuture<'a, ()> {
        Box::pin(async move {
            let node = self
                .node
                .as_ref()
                .ok_or_else(|| "p2p-net has not started".to_string())?;
            let binding = self
                .bindings
                .read()
                .map_err(|_| "p2p session bindings are poisoned".to_string())?
                .get(&sid)
                .cloned()
                .ok_or_else(|| "p2p session has no authenticated peer binding".to_string())?;
            node.send_message(
                binding.peer_id,
                ghost_protocol::session_topic(&sid),
                Uint8Array::from(packet),
            )
            .await
            .map_err(js_error)
        })
    }

    fn subscribe<'a>(&'a self, sid: [u8; 16]) -> P2pFuture<'a, Self::Subscription> {
        Box::pin(async move {
            let node = self
                .node
                .as_ref()
                .ok_or_else(|| "p2p-net has not started".to_string())?;
            let inner = node
                .subscribe(ghost_protocol::session_topic(&sid))
                .await
                .map_err(js_error)?;
            Ok(P2pNetSubscription {
                inner,
                bindings: self.bindings.clone(),
                sid,
            })
        })
    }

    fn shutdown<'a>(&'a mut self) -> P2pFuture<'a, ()> {
        Box::pin(async move {
            if let Some(mut node) = self.node.take() {
                node.shutdown().await.map_err(js_error)?;
            }
            self.bindings = Arc::new(RwLock::new(SessionPeerBindings::default()));
            Ok(())
        })
    }
}

fn network_id(value: &str) -> u32 {
    let hash = blake3::hash(value.as_bytes());
    u32::from_le_bytes(hash.as_bytes()[..4].try_into().expect("four-byte hash prefix"))
}

fn string_field(value: &JsValue, field: &str) -> Result<String, String> {
    Reflect::get(value, &JsValue::from_str(field))
        .map_err(js_error)?
        .as_string()
        .ok_or_else(|| format!("p2p-net message field {field} is not a string"))
}

fn js_error(value: JsValue) -> String {
    Reflect::get(&value, &JsValue::from_str("message"))
        .ok()
        .and_then(|value| value.as_string())
        .or_else(|| value.as_string())
        .unwrap_or_else(|| "p2p-net browser operation failed".into())
}

fn ensure_peer_suffix(address: String, peer_id: &str) -> String {
    if address.rsplit('/').next() == Some(peer_id) && address.contains("/p2p/") {
        address
    } else {
        format!("{}/p2p/{}", address.trim_end_matches('/'), peer_id)
    }
}
