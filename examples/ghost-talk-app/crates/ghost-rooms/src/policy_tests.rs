use crate::{ChannelPolicy, NewRoom, Role, RoomAccess, RoomMember, RoomService};

fn policy_room() -> crate::Room {
    RoomService::new_room(NewRoom {
        id: "room".into(),
        name: "Policy room".into(),
        owner_hydra_id: "owner".into(),
        owner_kaspa_address: "kaspa:owner".into(),
        owner_label: "Owner".into(),
        access: RoomAccess::Private,
        text_policy: ChannelPolicy::Interactive,
        audio_policy: ChannelPolicy::PresentersOnly,
        broadcast_enabled: true,
        pending_acceptance: false,
        revision: 0,
        members: vec![
            member("moderator", Role::Moderator),
            member("presenter", Role::Presenter),
            member("audience", Role::Audience),
        ],
        bans: Vec::new(),
    })
}

fn member(hydra_id: &str, role: Role) -> RoomMember {
    RoomMember {
        label: hydra_id.into(),
        kaspa_address: format!("kaspa:{hydra_id}"),
        hydra_handle: Some(hydra_id.into()),
        role,
        ..Default::default()
    }
}

#[test]
fn channel_policy_authorizes_owner_members_and_presenters() {
    let room = policy_room();
    assert!(room.can_send_text("owner"));
    assert!(room.can_send_text("audience"));
    assert!(!room.can_send_text("missing"));
    assert!(room.can_speak("owner"));
    assert!(room.can_speak("moderator"));
    assert!(room.can_speak("presenter"));
    assert!(!room.can_speak("audience"));
    assert!(room.can_broadcast("presenter"));
    assert!(!room.can_broadcast("audience"));
}
