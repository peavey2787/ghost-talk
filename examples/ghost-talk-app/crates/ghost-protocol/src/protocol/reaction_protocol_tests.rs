use super::{
    events::BASE64,
    kktp_types::{GhostReactionEvent, KktpDirection, KktpInnerMessage},
};
use base64::Engine as _;
use ghost_domain::reaction::ReactionKind;

#[test]
fn reaction_round_trip_and_validation() {
    let reaction = KktpInnerMessage::reaction(
        "11".repeat(16),
        "22".repeat(32),
        KktpDirection::AtoB,
        4,
        "33".repeat(16),
        GhostReactionEvent {
            target_message_id: "44".repeat(16),
            reaction: Some(ReactionKind::Love),
        },
    );
    let encoded = reaction.encode().expect("encode reaction");
    let decoded = KktpInnerMessage::decode(&encoded).expect("decode reaction");
    assert_eq!(decoded.reaction, reaction.reaction);

    let mut invalid = reaction;
    invalid.reaction.as_mut().unwrap().target_message_id = "bad".into();
    assert!(invalid.encode().is_err());
}

#[test]
fn reaction_can_seed_persistent_transport() {
    let mut reaction = KktpInnerMessage::reaction(
        "11".repeat(16),
        "22".repeat(32),
        KktpDirection::AtoB,
        0,
        "44".repeat(16),
        GhostReactionEvent {
            target_message_id: "55".repeat(16),
            reaction: Some(ReactionKind::Like),
        },
    );
    reaction.resume_seed_b64 = Some(BASE64.encode([8u8; 32]));
    assert!(reaction.encode().is_ok());
}
