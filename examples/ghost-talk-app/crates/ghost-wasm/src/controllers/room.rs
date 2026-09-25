pub(crate) const ROOM_WIRE_PREFIX: &str = "GTR2:";

mod creation;
mod invite;
mod lifecycle;
mod messaging;
mod moderation;
mod patches;
mod reactions;
mod transport;

pub(crate) use creation::{
    accept_bootstrap, accept_existing, bootstrap_room_name, create_room, decline_bootstrap,
};
pub(crate) use invite::invite_member;
pub(crate) use lifecycle::{complete_leave, disband, prepare_leave};
pub(crate) use messaging::{complete_send, prepare_send};
pub(crate) use moderation::{ban_member_command, remove_member, unban_member};
pub(crate) use patches::{
    address_chat_ids, append_chat, append_chats, append_room, append_tombstone, peer_chat_ids,
    room_patch, room_wallet_patch, union_chat_ids, wallet_patch,
};
pub(crate) use reactions::{complete_reaction, prepare_reaction};
pub(crate) use transport::{
    broadcast_room_wire_to_members, decode_room_wire, encode_room_wire,
    remove_pending_room_payloads, room_member_session_established, room_member_session_restoring,
    room_state_wire, send_room_wire_to_member, send_room_wire_to_route,
};
