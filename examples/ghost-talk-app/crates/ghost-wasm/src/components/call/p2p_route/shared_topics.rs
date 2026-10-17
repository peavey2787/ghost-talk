//! Consumers of the shared swarm topics for one node generation: the no-spend
//! mailbox (frames tagged for this wallet join the normal mailbox pipeline)
//! and the public room directory.

use ghost_p2p::P2pNetTopicSubscription;
use ghost_realtime::{MAILBOX_TOPIC, ROOM_DIRECTORY_TOPIC};
use wasm_bindgen_futures::spawn_local;

use super::{events::current, trace};
use crate::{components::call::CallRuntime, model::WalletLiveEvent, swarm_carrier};

type Handler = fn(&CallRuntime, &[u8]);

pub(super) async fn start(runtime: &CallRuntime, generation: u64) -> Result<(), String> {
    let node = runtime.p2p();
    let mailbox = node.subscribe_topic(MAILBOX_TOPIC).await?;
    let rooms = node.subscribe_topic(ROOM_DIRECTORY_TOPIC).await?;
    swarm_carrier::attach(node);
    spawn_local(consume(runtime.clone(), generation, mailbox, receive_mail));
    spawn_local(consume(runtime.clone(), generation, rooms, receive_room));
    Ok(())
}

async fn consume(
    runtime: CallRuntime,
    generation: u64,
    mut subscription: P2pNetTopicSubscription,
    handle: Handler,
) {
    while current(&runtime, generation) {
        match subscription.next().await {
            Ok(incoming) if current(&runtime, generation) => handle(&runtime, &incoming.packet),
            Ok(_) => return,
            Err(error) => return trace(&runtime, "shared-topic-closed", error),
        }
    }
}

fn receive_mail(runtime: &CallRuntime, packet: &[u8]) {
    if let Some(event) = mailbox_event(runtime, packet) {
        runtime.on_swarm_mailbox.emit(event);
    }
}

fn mailbox_event(runtime: &CallRuntime, packet: &[u8]) -> Option<WalletLiveEvent> {
    let profile = runtime.profile_ref.borrow();
    let wallet = profile.wallet.as_ref()?;
    let addresses: Vec<String> = wallet
        .public
        .receive_addresses
        .iter()
        .chain(&wallet.public.change_addresses)
        .cloned()
        .collect();
    let received_at_secs = (crate::now_ms() / 1000.0).max(0.0) as u64;
    let event = swarm_carrier::mailbox_event(packet, &addresses, received_at_secs)?;
    Some(WalletLiveEvent {
        profile_id: profile.id.clone(),
        snapshot: None,
        checkpoint: wallet.mailbox_checkpoint.clone(),
        mailbox: vec![event],
    })
}

fn receive_room(runtime: &CallRuntime, packet: &[u8]) {
    if let Some(announcement) = swarm_carrier::room_announcement(packet) {
        runtime.rooms.admit(announcement);
    }
}
