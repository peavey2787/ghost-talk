use ghost_chat::{Chat, ChatService, HydraSessionManager, Message};
use ghost_contacts::{Contact, ContactService};
use ghost_domain::{
    call::{CallEvent, CallManager, CallPhase, CallSignal, SignalDisposition},
    identity::PeerBinding,
};
use ghost_runtime::Profile;

fn peer(address: &str, hydra: &str) -> PeerBinding {
    PeerBinding::new(address.to_string(), hydra.to_string()).unwrap()
}

fn linked_chat(id: &str, label: &str, remote: &PeerBinding) -> Chat {
    ghost_chat::ChatService::new_direct_chat(ghost_chat::DirectChatSpec {
        id: id.into(),
        label: label.into(),
        contact_id: None,
        kaspa_address: remote.kaspa_address.clone(),
        hydra_handle: Some(remote.hydra_id.clone()),
        kns_name: None,
        dotk_name: None,
        verified_public: false,
    })
}

fn message(id: &str, direction: &str, body: &str) -> Message {
    Message {
        id: id.into(),
        wire_id: Some(id.into()),
        direction: direction.into(),
        body: body.into(),
        ..Default::default()
    }
}

fn connect_call(
    caller: &mut CallManager,
    receiver: &mut CallManager,
    call_id: &str,
    chat_id: &str,
    caller_peer: &PeerBinding,
    receiver_peer: &PeerBinding,
) {
    caller
        .begin_outgoing(
            call_id.into(),
            chat_id.into(),
            receiver_peer.kaspa_address.clone(),
            receiver_peer.hydra_id.clone(),
            "C".into(),
        )
        .unwrap();
    let request = CallSignal {
        call_id: call_id.into(),
        action: "request".into(),
        peer_address: caller_peer.kaspa_address.clone(),
        peer_hydra_id: caller_peer.hydra_id.clone(),
        peer_label: "A".into(),
    };
    assert_eq!(
        receiver.receive_signal(&request),
        SignalDisposition::Applied
    );
    receiver.attach_chat(call_id, chat_id.into()).unwrap();
    receiver.event(call_id, CallEvent::AcceptLocal).unwrap();
    caller.event(call_id, CallEvent::AcceptRemote).unwrap();
    caller
        .event(call_id, CallEvent::TransportConnected)
        .unwrap();
    receiver
        .event(call_id, CallEvent::TransportConnected)
        .unwrap();
    assert_eq!(caller.get(call_id).unwrap().phase, CallPhase::Connected);
    assert_eq!(receiver.get(call_id).unwrap().phase, CallPhase::Connected);
}

#[test]
fn saved_b_never_claims_unrelated_c_and_two_clients_survive_leave_call_restart() {
    let a_peer = peer("kaspatest:a", "hydra-a");
    let b_peer = peer("kaspatest:b", "hydra-b");
    let c_peer = peer("kaspatest:c", "hydra-c");

    let mut a = Profile::new("profile-a".into(), "A".into());
    let mut c = Profile::new("profile-c".into(), "C".into());
    ContactService::push(
        &mut a.contacts,
        Contact::authenticated("contact-b".into(), "B".into(), &b_peer),
    );

    assert!(ContactService::resolve_peer(&a.contacts, &c_peer).is_none());
    assert_eq!(
        ContactService::resolve_peer(&a.contacts, &b_peer).map(|contact| contact.id.as_str()),
        Some("contact-b")
    );

    ChatService::push(&mut a.chats, linked_chat("a-c-1", "kaspatest:c", &c_peer));
    ChatService::push(&mut c.chats, linked_chat("c-a-1", "A", &a_peer));
    HydraSessionManager::establish(&mut a.chats, "a-c-1", "sid-1".into(), "responder".into());
    HydraSessionManager::establish(&mut c.chats, "c-a-1", "sid-1".into(), "initiator".into());

    ChatService::record_message(&mut a.chats, "a-c-1", message("c-to-a", "in", "hello A"));
    ChatService::record_message(&mut c.chats, "c-a-1", message("a-to-c", "in", "hello C"));
    assert_eq!(
        ChatService::by_id(&a.chats, "a-c-1").unwrap().messages()[0].body,
        "hello A"
    );
    assert_eq!(
        ChatService::by_id(&c.chats, "c-a-1").unwrap().messages()[0].body,
        "hello C"
    );
    assert_eq!(
        ChatService::by_id(&a.chats, "a-c-1").unwrap().label,
        "kaspatest:c"
    );

    let mut a_calls = CallManager::default();
    let mut c_calls = CallManager::default();
    connect_call(
        &mut a_calls,
        &mut c_calls,
        "call-1",
        "a-c-1",
        &a_peer,
        &c_peer,
    );
    a_calls.event("call-1", CallEvent::SetMuted(true)).unwrap();
    assert!(a_calls.get("call-1").unwrap().muted);
    a_calls.event("call-1", CallEvent::SetMuted(false)).unwrap();
    assert!(!a_calls.get("call-1").unwrap().muted);
    a_calls.event("call-1", CallEvent::Hangup).unwrap();
    a_calls.event("call-1", CallEvent::CompleteEnd).unwrap();
    assert!(a_calls.visible().is_none());
    let remote_hangup = CallSignal {
        call_id: "call-1".into(),
        action: "hangup".into(),
        peer_address: a_peer.kaspa_address.clone(),
        peer_hydra_id: a_peer.hydra_id.clone(),
        peer_label: "A".into(),
    };
    assert_eq!(
        c_calls.receive_signal(&remote_hangup),
        SignalDisposition::Applied
    );
    c_calls.event("call-1", CallEvent::CompleteEnd).unwrap();

    connect_call(
        &mut a_calls,
        &mut c_calls,
        "call-2",
        "a-c-1",
        &a_peer,
        &c_peer,
    );
    let c_hangup = CallSignal {
        call_id: "call-2".into(),
        action: "hangup".into(),
        peer_address: c_peer.kaspa_address.clone(),
        peer_hydra_id: c_peer.hydra_id.clone(),
        peer_label: "C".into(),
    };
    c_calls.event("call-2", CallEvent::Hangup).unwrap();
    c_calls.event("call-2", CallEvent::CompleteEnd).unwrap();
    assert_eq!(
        a_calls.receive_signal(&c_hangup),
        SignalDisposition::Applied
    );
    a_calls.event("call-2", CallEvent::CompleteEnd).unwrap();

    assert!(ChatService::leave(&mut a.chats, "a-c-1"));
    assert!(ChatService::leave(&mut c.chats, "c-a-1"));
    ChatService::push(&mut a.chats, linked_chat("a-c-2", "kaspatest:c", &c_peer));
    ChatService::push(&mut c.chats, linked_chat("c-a-2", "A", &a_peer));
    HydraSessionManager::establish(&mut a.chats, "a-c-2", "sid-2".into(), "responder".into());
    HydraSessionManager::establish(&mut c.chats, "c-a-2", "sid-2".into(), "initiator".into());

    let a_json = serde_json::to_string(&a).unwrap();
    let c_json = serde_json::to_string(&c).unwrap();
    let mut a: Profile = serde_json::from_str(&a_json).unwrap();
    let mut c: Profile = serde_json::from_str(&c_json).unwrap();
    let restarted_a_calls = CallManager::default();
    let restarted_c_calls = CallManager::default();
    assert!(restarted_a_calls.active().is_none() && restarted_a_calls.visible().is_none());
    assert!(restarted_c_calls.active().is_none() && restarted_c_calls.visible().is_none());
    assert!(ContactService::resolve_peer(&a.contacts, &c_peer).is_none());
    assert_eq!(
        ContactService::resolve_peer(&a.contacts, &b_peer)
            .unwrap()
            .id,
        "contact-b"
    );
    assert_eq!(
        ChatService::by_id(&a.chats, "a-c-2").unwrap().label,
        "kaspatest:c"
    );

    ChatService::record_message(
        &mut a.chats,
        "a-c-2",
        message("post-restart-c", "in", "still C"),
    );
    ChatService::record_message(
        &mut c.chats,
        "c-a-2",
        message("post-restart-a", "in", "still A"),
    );
    assert_eq!(
        ChatService::by_id(&a.chats, "a-c-2")
            .unwrap()
            .messages()
            .last()
            .unwrap()
            .body,
        "still C"
    );
    assert_eq!(
        ChatService::by_id(&c.chats, "c-a-2")
            .unwrap()
            .messages()
            .last()
            .unwrap()
            .body,
        "still A"
    );
}
