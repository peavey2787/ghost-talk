use ghost_api::{MailboxEvent, WalletLiveEvent};
use ghost_kaspa::{LiveBlockEvent, LiveTransactionObservation, PortalFacade};
use kaspa_portal::network::{wrpc::block_added::OwnedBlockAddedNotification, NetworkApi};
use std::{cell::Cell, cell::RefCell, collections::HashMap, rc::Rc};
use wasm_bindgen_futures::spawn_local;

use crate::native::browser_host::support::debug;

const RECONNECT_DELAY_MS: u32 = 3_000;

/// One BlockAdded stream per profile over its own Portal notification socket.
/// The socket is replaced in place when it drops, so the stream survives
/// public-node disconnects.
struct BrowserLiveStream {
    portal: Rc<RefCell<PortalFacade>>,
    endpoint: String,
    alive: Rc<Cell<bool>>,
}

/// What the notification task needs to reopen its socket.
struct StreamTarget {
    profile_id: String,
    network: String,
    endpoint: String,
    portal: Rc<RefCell<PortalFacade>>,
    alive: Rc<Cell<bool>>,
}

thread_local! {
    static LIVE_STREAMS: RefCell<HashMap<String, BrowserLiveStream>> = RefCell::new(HashMap::new());
}

pub(in crate::native::browser_host) async fn start(
    profile_id: &str,
    public: &ghost_kaspa::wallet::WalletPublic,
    portal: &PortalFacade,
) -> Result<(), String> {
    if live_matches(profile_id, portal)? {
        return Ok(());
    }
    stop(profile_id).await;
    let endpoint = portal.endpoint()?;
    let (stream, api) = open_stream(&public.network, &endpoint).await?;
    let target = StreamTarget {
        profile_id: profile_id.to_owned(),
        network: public.network.clone(),
        endpoint: endpoint.clone(),
        portal: Rc::new(RefCell::new(stream)),
        alive: Rc::new(Cell::new(true)),
    };
    LIVE_STREAMS.with(|streams| {
        streams.borrow_mut().insert(
            profile_id.to_owned(),
            BrowserLiveStream {
                portal: Rc::clone(&target.portal),
                endpoint,
                alive: Rc::clone(&target.alive),
            },
        );
    });
    spawn_local(run_stream(target, api));
    Ok(())
}

async fn open_stream(network: &str, endpoint: &str) -> Result<(PortalFacade, NetworkApi), String> {
    let stream = PortalFacade::connect(network, endpoint).await?;
    let api = stream.network_api()?;
    if let Err(error) = api.subscribe_block_added().await {
        stream.disconnect();
        return Err(format!("Kaspa BlockAdded subscribe failed: {error}"));
    }
    Ok((stream, api))
}

pub(in crate::native::browser_host) async fn stop(profile_id: &str) {
    let stream = LIVE_STREAMS.with(|streams| streams.borrow_mut().remove(profile_id));
    if let Some(stream) = stream {
        stream.alive.set(false);
        stream.portal.borrow().disconnect();
    }
}

fn live_matches(profile_id: &str, portal: &PortalFacade) -> Result<bool, String> {
    let endpoint = portal.endpoint()?;
    Ok(LIVE_STREAMS.with(|streams| {
        streams
            .borrow()
            .get(profile_id)
            .is_some_and(|stream| stream.endpoint == endpoint)
    }))
}

/// Deliver blocks until the stream is stopped, reopening a dropped socket.
async fn run_stream(target: StreamTarget, mut api: NetworkApi) {
    let mut blocks = 0u64;
    while target.alive.get() {
        let closed = pump_blocks(&target, &api, &mut blocks).await;
        if !target.alive.get() {
            return;
        }
        debug::record(
            "warn",
            "kaspa",
            "live-stream-closed",
            format!("blocks={blocks} {closed}"),
        );
        match reopen(&target).await {
            Some(next) => api = next,
            None => return,
        }
    }
}

async fn pump_blocks(target: &StreamTarget, api: &NetworkApi, blocks: &mut u64) -> String {
    loop {
        let block = match api.next_block_added().await {
            Ok(block) => block,
            Err(error) => return error.to_string(),
        };
        if !target.alive.get() {
            return "stopped".into();
        }
        *blocks += 1;
        let event = live_block_event(block);
        trace_block(*blocks, &event);
        handle_live_block(&target.profile_id, &target.network, event);
    }
}

/// Reopen the socket with a fixed backoff; `None` once the stream is stopped.
async fn reopen(target: &StreamTarget) -> Option<NetworkApi> {
    while target.alive.get() {
        gloo_timers::future::TimeoutFuture::new(RECONNECT_DELAY_MS).await;
        if !target.alive.get() {
            return None;
        }
        match open_stream(&target.network, &target.endpoint).await {
            Ok((stream, api)) => return adopt(target, stream, api),
            Err(error) => debug::record("warn", "kaspa", "live-stream-reopen-failed", error),
        }
    }
    None
}

/// Swap in a reopened socket, unless the stream was stopped meanwhile.
fn adopt(target: &StreamTarget, stream: PortalFacade, api: NetworkApi) -> Option<NetworkApi> {
    if !target.alive.get() {
        stream.disconnect();
        return None;
    }
    target.portal.replace(stream).disconnect();
    debug::record("info", "kaspa", "live-stream-reopened", String::new());
    Some(api)
}

/// Record carrier-bearing blocks (and a periodic heartbeat) for protocol debug.
fn trace_block(blocks: u64, event: &LiveBlockEvent) {
    if event.observations.is_empty() && blocks % 600 != 1 {
        return;
    }
    let details = format!(
        "blocks={blocks} daa={} carriers={}",
        event.daa_score,
        event.observations.len()
    );
    debug::record("info", "kaspa", "live-block", details);
}

fn live_block_event(block: OwnedBlockAddedNotification) -> LiveBlockEvent {
    let block_hash = block.block_hash;
    let daa_score = block.daa_score;
    let transaction_count = block.transactions.len();
    let observations = block
        .transactions
        .into_iter()
        .enumerate()
        .filter(|(_, transaction)| ghost_kaspa::is_live_ghost_carrier(&transaction.payload))
        .map(|(index, transaction)| LiveTransactionObservation {
            txid: transaction.transaction_id.unwrap_or_else(|| {
                ghost_kaspa::fallback_live_event_id(&block_hash, index, &transaction.payload)
            }),
            daa_score,
            payload: transaction.payload,
        })
        .collect();
    LiveBlockEvent {
        block_hash,
        daa_score,
        transaction_count,
        observations,
    }
}

fn handle_live_block(profile_id: &str, network: &str, event: LiveBlockEvent) {
    if event.observations.is_empty() {
        return;
    }
    super::peer::ingest_live(profile_id, network, &event.observations, event.daa_score);
    let mailbox = event
        .observations
        .into_iter()
        .map(|observation| MailboxEvent {
            transaction_id: observation.txid,
            blue_score: observation.daa_score.to_string(),
            payload_hex: hex::encode(observation.payload),
            block_time: None,
        })
        .collect();
    let live = WalletLiveEvent {
        profile_id: profile_id.to_owned(),
        snapshot: None,
        checkpoint: event.daa_score.to_string(),
        mailbox,
    };
    if let Ok(payload) = serde_json::to_value(live) {
        crate::native::events::emit_browser("ghost://wallet-live", payload);
    }
}
