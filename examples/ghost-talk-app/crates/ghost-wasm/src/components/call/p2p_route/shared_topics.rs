//! Consumers of the shared swarm topics for one node generation: the no-spend
//! mailbox (frames tagged for this wallet join the normal mailbox pipeline,
//! missed-mail queries are answered from held mail) and the public room
//! directory. Shortly after start the node asks the swarm for missed mail.

use ghost_p2p::P2pNetTopicSubscription;
use ghost_protocol::GTSQ_MAGIC;
use ghost_realtime::{MAILBOX_TOPIC, ROOM_DIRECTORY_TOPIC};
use wasm_bindgen_futures::spawn_local;

use super::{events::current, trace};
use crate::{components::call::CallRuntime, model::WalletLiveEvent, swarm_carrier};

type Handler = fn(&CallRuntime, &[u8]);

/// Keep the held-mail policy in step with the profile's settings.
#[yew::hook]
pub(in crate::components::call) fn use_retention_policy(profile: &crate::model::Profile) {
    yew::use_effect_with(profile.settings.swarm_retention, |policy| {
        swarm_carrier::set_policy(*policy);
    });
}

pub(super) async fn start(runtime: &CallRuntime, generation: u64) -> Result<(), String> {
    let node = runtime.p2p();
    let mailbox = node.subscribe_topic(MAILBOX_TOPIC).await?;
    let rooms = node.subscribe_topic(ROOM_DIRECTORY_TOPIC).await?;
    swarm_carrier::attach(node);
    spawn_local(consume(runtime.clone(), generation, mailbox, receive_mail));
    spawn_local(consume(runtime.clone(), generation, rooms, receive_room));
    spawn_local(ask_for_missed_mail(runtime.clone(), generation));
    Ok(())
}

async fn ask_for_missed_mail(runtime: CallRuntime, generation: u64) {
    for delay in swarm_carrier::QUERY_DELAYS_MS {
        gloo_timers::future::TimeoutFuture::new(delay).await;
        if !current(&runtime, generation) {
            return;
        }
        let query = swarm_carrier::missed_mail_query();
        if let Err(error) = swarm_carrier::publish(MAILBOX_TOPIC, &query).await {
            trace(&runtime, "missed-mail-query-failed", error);
        }
    }
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
    if packet.starts_with(&GTSQ_MAGIC) {
        let (runtime, query) = (runtime.clone(), packet.to_vec());
        spawn_local(async move {
            if let Err(error) = swarm_carrier::answer(&query).await {
                trace(&runtime, "missed-mail-reply-failed", error);
            }
        });
        return;
    }
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
    runtime.rooms.admit_packet(packet);
}
