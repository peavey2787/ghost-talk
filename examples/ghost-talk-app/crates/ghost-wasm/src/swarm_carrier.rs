//! p2p-net as the carrier for the shared swarm topics: no-spend mailbox frames
//! (GTSM) and the public room directory (GTRA, consumed by the app's room directory). The call runtime owns the node
//! and attaches it here while it runs; mailbox sends and the room announcer
//! publish through it, and its topic consumers read through `inbound`.
//! `retention` holds seen mail for peers that were briefly offline.
#![cfg(target_arch = "wasm32")]

mod inbound;
mod outbound;
mod retention;

pub(crate) use inbound::mailbox_event;
pub(crate) use outbound::{attach, deliver, detach, publish};
pub(crate) use retention::{answer, missed_mail_query, set_policy, QUERY_DELAYS_MS};
