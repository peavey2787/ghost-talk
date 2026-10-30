use std::{cell::RefCell, rc::Rc};

use ghost_realtime::{
    session_topic, GhostP2pTransport, LocalP2pBinding, P2pFuture, P2pIncoming, P2pStartConfig,
    P2pSubscription, SessionPeerBinding, SessionPeerBindings,
};
use js_sys::{Reflect, Uint8Array};
use p2p_net::wasm::{WasmEventSubscription, WasmNode, WasmSubscription};
use wasm_bindgen::JsValue;

use crate::events::{NodeEventWire, P2pNetEvent};
use crate::incoming::admit;
use crate::node::{js_error, start_node};

/// Shutdown waits up to 60 s for in-flight operations to release the node.
const SHUTDOWN_POLL_MS: u32 = 250;
const SHUTDOWN_POLLS: u32 = 240;

type SharedBindings = Rc<RefCell<SessionPeerBindings>>;

/// Ghost Talk's browser realtime transport, delegating every network concern
/// to an in-process p2p-net `WasmNode`. Clones share one node and one binding
/// table, so a caller can clone the handle out of UI state instead of holding
/// a borrow across an await.
#[derive(Clone, Default)]
pub struct P2pNetTransport {
    node: Rc<RefCell<Option<Rc<WasmNode>>>>,
    bindings: SharedBindings,
}

pub struct P2pNetSubscription {
    inner: WasmSubscription,
    bindings: SharedBindings,
    sid: [u8; 16],
}

pub struct P2pNetEventSubscription {
    inner: WasmEventSubscription,
}

/// A shared gossip topic (swarm mailbox, room directory). Deliveries carry the
/// authenticated source peer; the payload's own signatures decide admission.
pub struct P2pNetTopicSubscription {
    inner: WasmSubscription,
}

impl P2pNetTransport {
    fn node(&self) -> Result<Rc<WasmNode>, String> {
        self.node
            .borrow()
            .clone()
            .ok_or_else(|| "p2p-net has not started".to_string())
    }

    pub fn subscribe_events(&self) -> Result<P2pNetEventSubscription, String> {
        let inner = self.node()?.subscribe_events();
        Ok(P2pNetEventSubscription { inner })
    }

    /// Gossip `payload` to every subscriber of a shared `topic`.
    pub async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), String> {
        let node = self.node()?;
        node.broadcast(topic.to_owned(), Uint8Array::from(payload))
            .await
            .map_err(js_error)
    }

    pub async fn subscribe_topic(&self, topic: &str) -> Result<P2pNetTopicSubscription, String> {
        let node = self.node()?;
        let inner = node.subscribe(topic.to_owned()).await.map_err(js_error)?;
        Ok(P2pNetTopicSubscription { inner })
    }

    /// p2p-net's most recent application dial failure, for redial traces.
    pub async fn last_dial_error(&self) -> Option<String> {
        let json = self.node().ok()?.snapshot().await.ok()?;
        let snapshot: serde_json::Value = serde_json::from_str(&json).ok()?;
        snapshot["last_application_dial_error"]
            .as_str()
            .map(str::to_owned)
    }

    pub fn has_bound_peer(&self, peer_id: &str) -> bool {
        self.bindings.borrow().contains_peer(peer_id)
    }
}

impl P2pNetEventSubscription {
    pub async fn next(&mut self) -> Result<P2pNetEvent, String> {
        let value = self.inner.recv().await.map_err(js_error)?;
        let event: NodeEventWire = serde_wasm_bindgen::from_value(value)
            .map_err(|error| format!("invalid p2p-net node event: {error}"))?;
        Ok(event.into())
    }
}

impl P2pNetTopicSubscription {
    pub async fn next(&mut self) -> Result<P2pIncoming, String> {
        let value = self.inner.recv().await.map_err(js_error)?;
        read_incoming(&value)
    }
}

impl P2pSubscription for P2pNetSubscription {
    fn next<'a>(&'a mut self) -> P2pFuture<'a, Option<P2pIncoming>> {
        Box::pin(async move {
            loop {
                let value = self.inner.recv().await.map_err(js_error)?;
                let incoming = read_incoming(&value)?;
                if let Some(admitted) = admit(&self.sid, &self.bindings.borrow(), incoming) {
                    return Ok(Some(admitted));
                }
            }
        })
    }
}

impl GhostP2pTransport for P2pNetTransport {
    type Subscription = P2pNetSubscription;

    fn start<'a>(&'a mut self, config: P2pStartConfig) -> P2pFuture<'a, LocalP2pBinding> {
        Box::pin(async move {
            if self.node.borrow().is_none() {
                let node = start_node(&config).await?;
                *self.node.borrow_mut() = Some(Rc::new(node));
            }
            self.local_binding().await
        })
    }

    fn local_binding<'a>(&'a self) -> P2pFuture<'a, LocalP2pBinding> {
        Box::pin(async move {
            let value = self.node()?.local_binding().await.map_err(js_error)?;
            let binding: LocalP2pBinding = serde_wasm_bindgen::from_value(value)
                .map_err(|error| format!("invalid p2p-net local binding: {error}"))?;
            Ok(binding.with_peer_suffixes())
        })
    }

    fn bind_session<'a>(&'a mut self, binding: SessionPeerBinding) -> P2pFuture<'a, ()> {
        Box::pin(async move { self.bindings.borrow_mut().bind(binding) })
    }

    fn connect<'a>(&'a mut self, address: &'a str) -> P2pFuture<'a, ()> {
        Box::pin(async move {
            let node = self.node()?;
            node.connect_peer(address.to_owned())
                .await
                .map_err(js_error)
        })
    }

    fn send<'a>(&'a self, sid: [u8; 16], packet: &'a [u8]) -> P2pFuture<'a, ()> {
        Box::pin(async move {
            let peer_id = self
                .bindings
                .borrow()
                .get(&sid)
                .map(|binding| binding.peer_id.clone())
                .ok_or_else(|| "p2p session has no authenticated peer binding".to_string())?;
            let payload = Uint8Array::from(packet);
            let node = self.node()?;
            node.send_message(peer_id, session_topic(&sid), payload)
                .await
                .map_err(js_error)
        })
    }

    fn subscribe<'a>(&'a self, sid: [u8; 16]) -> P2pFuture<'a, Self::Subscription> {
        Box::pin(async move {
            let node = self.node()?;
            let inner = node
                .subscribe(session_topic(&sid))
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
            *self.bindings.borrow_mut() = SessionPeerBindings::default();
            let node = self.node.borrow_mut().take();
            match node {
                Some(node) => shutdown_when_released(node).await,
                None => Ok(()),
            }
        })
    }
}

/// In-flight operations hold clones of the node; wait for them to finish so
/// the node really stops. Dropping it un-shut would leave its swarm running,
/// still holding relay circuits and the profile lock.
async fn shutdown_when_released(mut node: Rc<WasmNode>) -> Result<(), String> {
    for _ in 0..SHUTDOWN_POLLS {
        match Rc::try_unwrap(node) {
            Ok(mut owned) => return owned.shutdown().await.map_err(js_error),
            Err(shared) => node = shared,
        }
        gloo_timers::future::TimeoutFuture::new(SHUTDOWN_POLL_MS).await;
    }
    Err("p2p-net stayed busy; shutdown abandoned".into())
}

fn read_incoming(value: &JsValue) -> Result<P2pIncoming, String> {
    let field = |name: &str| Reflect::get(value, &JsValue::from_str(name)).map_err(js_error);
    let text = |name: &str| {
        field(name)?
            .as_string()
            .ok_or_else(|| format!("p2p-net message field {name} is not a string"))
    };
    Ok(P2pIncoming {
        source_peer_id: text("sourcePeerId")?,
        topic: text("topic")?,
        packet: Uint8Array::new(&field("payload")?).to_vec(),
    })
}
