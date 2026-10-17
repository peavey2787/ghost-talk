//! p2p-net as the carrier for the shared swarm topics: no-spend mailbox frames
//! (GTSM) and the public room directory (GTRA). The call runtime owns the node
//! and attaches it here while it runs; mailbox sends and the room announcer
//! publish through it, and its topic consumers read through `inbound`.
#![cfg(target_arch = "wasm32")]

mod inbound;
mod outbound;

pub(crate) use inbound::{mailbox_event, room_announcement};
pub(crate) use outbound::{attach, deliver, detach, publish};
