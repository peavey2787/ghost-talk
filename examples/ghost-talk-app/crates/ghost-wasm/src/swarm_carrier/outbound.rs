use std::cell::RefCell;

use ghost_api::SwarmDelivery;
use ghost_p2p::P2pNetTransport;
use ghost_protocol::SwarmMailboxPacket;
use ghost_realtime::MAILBOX_TOPIC;

thread_local! {
    static NODE: RefCell<Option<P2pNetTransport>> = const { RefCell::new(None) };
}

pub(crate) fn attach(node: P2pNetTransport) {
    NODE.with(|slot| *slot.borrow_mut() = Some(node));
}

pub(crate) fn detach() {
    NODE.with(|slot| slot.borrow_mut().take());
}

fn node() -> Result<P2pNetTransport, String> {
    NODE.with(|slot| slot.borrow().clone()).ok_or_else(|| {
        "The swarm is offline, so this no-spend message could not be sent. Wait for swarm peers or switch to Kaspa mode."
            .to_string()
    })
}

pub(crate) async fn publish(topic: &str, payload: &[u8]) -> Result<(), String> {
    node()?.publish(topic, payload).await
}

/// Publish every frame of a no-spend mailbox send, tagged for its recipient.
pub(crate) async fn deliver(delivery: &SwarmDelivery) -> Result<(), String> {
    let node = attached_node().await?;
    let sent_at_secs = (crate::now_ms() / 1000.0).max(0.0) as u64;
    for frame_hex in &delivery.frames_hex {
        let frame =
            hex::decode(frame_hex).map_err(|_| "swarm mailbox frame is not hex".to_string())?;
        let packet =
            SwarmMailboxPacket::for_destination(&delivery.destination, frame, sent_at_secs)?;
        super::retention::hold(&packet);
        publish_until_peered(&node, &packet.encode()).await?;
    }
    Ok(())
}

/// Attempts, `PUBLISH_RETRY_MS` apart, to find the node and to publish each
/// frame: a node that has just reached its relay may not yet know which peers
/// share the topic.
const PUBLISH_ATTEMPTS: u32 = 20;
const PUBLISH_RETRY_MS: u32 = 500;

/// The node attaches a moment after a profile unlocks or switches modes; a
/// send made in that window waits for it instead of failing.
async fn attached_node() -> Result<P2pNetTransport, String> {
    for _ in 1..PUBLISH_ATTEMPTS {
        if let Ok(node) = node() {
            return Ok(node);
        }
        gloo_timers::future::TimeoutFuture::new(PUBLISH_RETRY_MS).await;
    }
    node()
}

async fn publish_until_peered(node: &P2pNetTransport, packet: &[u8]) -> Result<(), String> {
    let mut attempt = 1;
    loop {
        match node.publish(MAILBOX_TOPIC, packet).await {
            Err(_) if attempt < PUBLISH_ATTEMPTS => {
                attempt += 1;
                gloo_timers::future::TimeoutFuture::new(PUBLISH_RETRY_MS).await;
            }
            result => return result,
        }
    }
}
