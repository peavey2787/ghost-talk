use ghost_api::{MailboxEvent, WalletLiveEvent};
use ghost_kaspa::{LiveBlockEvent, LiveTransactionObservation, PortalFacade};
use kaspa_wrpc_client::prelude::*;
use std::{cell::RefCell, collections::HashMap, str::FromStr, sync::Arc};
use wasm_bindgen_futures::spawn_local;
use workflow_core::channel::Channel;

struct BrowserLiveStream {
    client: Arc<KaspaRpcClient>,
    listener_id: ListenerId,
    endpoint: String,
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
    let (client, notifications, listener_id) = connect_block_stream(&public.network, &endpoint).await?;
    remember_stream(profile_id, &endpoint, client.clone(), listener_id);
    spawn_notification_task(profile_id.to_owned(), public.network.clone(), client, notifications);
    Ok(())
}

async fn connect_block_stream(
    network: &str,
    endpoint: &str,
) -> Result<(Arc<KaspaRpcClient>, Channel<Notification>, ListenerId), String> {
    let network_id = NetworkId::from_str(network).map_err(|error| error.to_string())?;
    let client = Arc::new(KaspaRpcClient::new_with_args(
        WrpcEncoding::Borsh,
        Some(endpoint),
        None,
        Some(network_id),
        None,
    ).map_err(|error| error.to_string())?);
    client.connect(Some(ConnectOptions {
        block_async_connect: true,
        ..Default::default()
    })).await.map_err(|error| format!("Kaspa BlockAdded WebSocket connect failed: {error}"))?;
    let notifications = Channel::<Notification>::unbounded();
    let listener_id = client.register_new_listener(ChannelConnection::new(
        "ghost-talk-web-live-blocks",
        notifications.sender.clone(),
        ChannelType::Persistent,
    ));
    client.start_notify(listener_id, Scope::BlockAdded(BlockAddedScope {})).await
        .map_err(|error| format!("Kaspa BlockAdded subscribe failed: {error}"))?;
    Ok((client, notifications, listener_id))
}

fn remember_stream(
    profile_id: &str,
    endpoint: &str,
    client: Arc<KaspaRpcClient>,
    listener_id: ListenerId,
) {
    LIVE_STREAMS.with(|streams| {
        streams.borrow_mut().insert(profile_id.to_owned(), BrowserLiveStream {
            client,
            listener_id,
            endpoint: endpoint.to_owned(),
        });
    });
}

pub(in crate::native::browser_host) async fn stop(profile_id: &str) {
    let stream = LIVE_STREAMS.with(|streams| streams.borrow_mut().remove(profile_id));
    let Some(stream) = stream else { return };
    let _ = stream
        .client
        .stop_notify(stream.listener_id, Scope::BlockAdded(BlockAddedScope {}))
        .await;
    let _ = stream.client.unregister_listener(stream.listener_id).await;
    let _ = stream.client.disconnect().await;
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

fn spawn_notification_task(
    profile_id: String,
    network: String,
    client: Arc<KaspaRpcClient>,
    notifications: Channel<Notification>,
) {
    spawn_local(async move {
        while let Ok(notification) = notifications.receiver.recv().await {
            if !stream_is_current(&profile_id, &client) {
                break;
            }
            let Notification::BlockAdded(added) = notification else {
                continue;
            };
            let event = live_block_event(&added.block);
            handle_live_block(&profile_id, &network, event);
        }
    });
}

fn stream_is_current(profile_id: &str, client: &Arc<KaspaRpcClient>) -> bool {
    LIVE_STREAMS.with(|streams| {
        streams
            .borrow()
            .get(profile_id)
            .is_some_and(|stream| Arc::ptr_eq(&stream.client, client))
    })
}

fn live_block_event(block: &RpcBlock) -> LiveBlockEvent {
    let block_hash = block
        .verbose_data
        .as_ref()
        .map(|verbose| verbose.hash.to_string())
        .unwrap_or_else(|| format!("daa-{}", block.header.daa_score));
    let daa_score = block.header.daa_score;
    let transaction_count = block.transactions.len();
    let observations = block
        .transactions
        .iter()
        .enumerate()
        .filter_map(|(index, transaction)| {
            ghost_kaspa::is_live_ghost_carrier(&transaction.payload).then(|| LiveTransactionObservation {
                txid: transaction
                    .verbose_data
                    .as_ref()
                    .map(|verbose| verbose.transaction_id.to_string())
                    .unwrap_or_else(|| {
                        ghost_kaspa::fallback_live_event_id(&block_hash, index, &transaction.payload)
                    }),
                daa_score,
                payload: transaction.payload.clone(),
            })
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
