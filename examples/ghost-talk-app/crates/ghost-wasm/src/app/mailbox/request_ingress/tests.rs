use super::*;
use crate::model::Profile;

fn incoming(address: &str, hydra: &str) -> HydraIncomingRequestProjection {
    HydraIncomingRequestProjection {
        request_id: "0123456789abcdef0123456789abcdef".into(),
        peer_address: address.into(),
        local_address: "kaspatest:local".into(),
        peer_label: "New peer".into(),
        peer_hydra_id: hydra.into(),
        signed_request_hex: "00".into(),
        room_invite: None,
        call_id: None,
        call_action: None,
    }
}

fn saved_contact(address: &str, hydra: Option<&str>) -> Contact {
    ghost_contacts::ContactService::new_contact(
        "saved-contact".into(),
        "Old saved contact".into(),
        address.into(),
        hydra.map(str::to_string),
    )
}

#[test]
fn saved_contact_requires_both_authenticated_identifiers_when_hydra_is_known() {
    let mut profile = Profile::new("p".into(), "Local".into());
    ghost_contacts::ContactService::push(
        &mut profile.contacts,
        saved_contact("kaspatest:old", Some("old-hydra")),
    );

    upsert_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        incoming("kaspatest:new", "old-hydra"),
    );

    let chat = &profile.chats[0];
    assert_eq!(chat.contact_id(), None);
    assert_eq!(chat.label, "kaspatest:new");
    assert_eq!(chat.peer_kaspa_address(), Some("kaspatest:new"));
    assert_eq!(chat.peer_hydra_handle(), Some("old-hydra"));
}

#[test]
fn saved_contact_does_not_match_same_address_with_different_hydra_identity() {
    let mut profile = Profile::new("p".into(), "Local".into());
    ghost_contacts::ContactService::push(
        &mut profile.contacts,
        saved_contact("kaspatest:same", Some("old-hydra")),
    );

    upsert_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        incoming("kaspatest:same", "new-hydra"),
    );

    let chat = &profile.chats[0];
    assert_eq!(chat.contact_id(), None);
    assert_eq!(chat.label, "kaspatest:same");
}

#[test]
fn exact_saved_binding_still_uses_saved_contact_label() {
    let mut profile = Profile::new("p".into(), "Local".into());
    ghost_contacts::ContactService::push(
        &mut profile.contacts,
        saved_contact("kaspatest:same", Some("same-hydra")),
    );

    upsert_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        incoming("kaspatest:same", "same-hydra"),
    );

    let chat = &profile.chats[0];
    assert_eq!(chat.contact_id(), Some("saved-contact"));
    assert_eq!(chat.label, "Old saved contact");
}

#[test]
fn incomplete_saved_binding_does_not_auto_match() {
    let mut profile = Profile::new("p".into(), "Local".into());
    ghost_contacts::ContactService::push(
        &mut profile.contacts,
        saved_contact("kaspatest:incomplete", None),
    );

    upsert_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        incoming("KASPATEST:INCOMPLETE", "new-hydra"),
    );

    let chat = &profile.chats[0];
    assert_eq!(chat.contact_id(), None);
    assert_eq!(chat.label, "KASPATEST:INCOMPLETE");
}

#[test]
fn same_request_id_from_different_peer_does_not_reuse_old_thread() {
    let mut profile = Profile::new("p".into(), "Local".into());
    let first = incoming("kaspatest:first", "hydra-first");
    upsert_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        first.clone(),
    );
    assert_eq!(profile.chats.len(), 1);

    let mut second = incoming("kaspatest:second", "hydra-second");
    second.request_id = first.request_id;
    upsert_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        second,
    );

    assert_eq!(profile.chats.len(), 2);
    assert_eq!(profile.chats[0].label, "kaspatest:second");
    assert_eq!(profile.chats[1].label, "kaspatest:first");
}
