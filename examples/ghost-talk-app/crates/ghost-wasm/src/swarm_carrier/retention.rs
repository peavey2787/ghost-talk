//! Store-and-forward for no-spend mail. Every GTSM packet this client sees
//! or sends is held (within the profile's retention policy) so a peer that
//! was briefly offline can ask the swarm for what it missed. Queries name no
//! recipient; each receiver keeps only the packets tagged for its addresses.

use std::cell::{Cell, RefCell};

use ghost_domain::settings::SwarmRetention;
use ghost_protocol::{SwarmMailQuery, SwarmMailStore, SwarmMailboxPacket};
use ghost_realtime::MAILBOX_TOPIC;
use gloo_timers::future::TimeoutFuture;

/// Replies wait a random 0.5–3 s so holders that answer later can skip the
/// packets an earlier holder already replayed.
const REPLY_MIN_DELAY_MS: f64 = 500.0;
const REPLY_JITTER_MS: f64 = 2_500.0;
/// Missed-mail queries after the node starts: once peers are reachable, and
/// once more after the mesh has settled.
pub(crate) const QUERY_DELAYS_MS: [u32; 2] = [5_000, 30_000];

thread_local! {
    static STORE: RefCell<SwarmMailStore> = RefCell::new(SwarmMailStore::default());
    static POLICY: Cell<SwarmRetention> = Cell::new(SwarmRetention::default());
}

fn now_ms() -> u64 {
    crate::now_ms().max(0.0) as u64
}

pub(crate) fn set_policy(policy: SwarmRetention) {
    POLICY.with(|current| current.set(policy.bounded()));
}

fn policy() -> SwarmRetention {
    POLICY.with(Cell::get)
}

/// Hold one packet seen on, or published to, the mailbox topic.
pub(crate) fn hold(packet: &SwarmMailboxPacket) {
    STORE.with(|store| store.borrow_mut().hold(packet, now_ms(), policy()));
}

/// The query this client publishes: everything held within its own window.
pub(crate) fn missed_mail_query() -> Vec<u8> {
    let since_secs = (now_ms() / 1000).saturating_sub(policy().max_age_secs());
    SwarmMailQuery { since_secs }.encode()
}

/// Answer a query heard on the mailbox topic after a random delay.
pub(crate) async fn answer(query: &[u8]) -> Result<(), String> {
    let Ok(query) = SwarmMailQuery::decode(query) else {
        return Ok(());
    };
    let asked_at_ms = now_ms();
    let delay = REPLY_MIN_DELAY_MS + js_sys::Math::random() * REPLY_JITTER_MS;
    TimeoutFuture::new(delay as u32).await;
    let replay = STORE.with(|store| {
        store
            .borrow_mut()
            .reply(query, asked_at_ms, now_ms(), policy())
    });
    for packet in replay {
        super::publish(MAILBOX_TOPIC, &packet).await?;
    }
    Ok(())
}
