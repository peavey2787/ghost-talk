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
    let node = node()?;
    for frame_hex in &delivery.frames_hex {
        let frame =
            hex::decode(frame_hex).map_err(|_| "swarm mailbox frame is not hex".to_string())?;
        let packet = SwarmMailboxPacket::for_destination(&delivery.destination, frame)?;
        node.publish(MAILBOX_TOPIC, &packet.encode()).await?;
    }
    Ok(())
}
