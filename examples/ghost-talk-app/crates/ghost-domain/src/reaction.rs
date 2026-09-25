use serde::{Deserialize, Serialize};

/// Supported Ghost-native message reactions.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReactionKind {
    Like,
    Love,
    Haha,
    Sad,
    Surprised,
    Dislike,
    Disgust,
    Angry,
    Fear,
}

impl ReactionKind {
    /// Stable presentation glyph for this reaction.
    pub const fn emoji(self) -> &'static str {
        const EMOJIS: [&str; 9] = ["👍", "❤️", "😂", "😢", "😮", "👎", "🤢", "😡", "😨"];
        EMOJIS[self as usize]
    }

    /// User-facing label for this reaction.
    pub const fn label(self) -> &'static str {
        const LABELS: [&str; 9] = [
            "Like",
            "Love",
            "Haha",
            "Sad",
            "Surprised",
            "Dislike",
            "Disgust",
            "Angry",
            "Fear",
        ];
        LABELS[self as usize]
    }

    /// All supported reactions in the canonical picker order.
    pub const ALL: [Self; 9] = [
        Self::Like,
        Self::Love,
        Self::Haha,
        Self::Sad,
        Self::Surprised,
        Self::Dislike,
        Self::Disgust,
        Self::Angry,
        Self::Fear,
    ];
}

/// One actor's current reaction to a message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageReaction {
    pub actor_id: String,
    pub kind: ReactionKind,
}

/// Returns one actor's current reaction, if any.
pub fn actor_reaction(reactions: &[MessageReaction], actor_id: &str) -> Option<ReactionKind> {
    reactions
        .iter()
        .find(|entry| entry.actor_id == actor_id)
        .map(|entry| entry.kind)
}

/// Three-way merges reactions while preserving independent actor changes.
pub fn merge_reactions(
    before: &[MessageReaction],
    changed: &[MessageReaction],
    latest: &[MessageReaction],
) -> Vec<MessageReaction> {
    let mut merged = changed.to_vec();
    let mut actors = before
        .iter()
        .map(|entry| entry.actor_id.as_str())
        .collect::<Vec<_>>();
    for entry in latest {
        if !actors.contains(&entry.actor_id.as_str()) {
            actors.push(entry.actor_id.as_str());
        }
    }
    for actor in actors {
        let baseline = actor_reaction(before, actor);
        let local = actor_reaction(&merged, actor);
        let concurrent = actor_reaction(latest, actor);
        if local == baseline && concurrent != baseline {
            let _ = set_actor_reaction(&mut merged, actor, concurrent);
        }
    }
    merged
}

/// Applies one-reaction-per-actor semantics to a reaction list.
pub fn set_actor_reaction(
    reactions: &mut Vec<MessageReaction>,
    actor_id: &str,
    kind: Option<ReactionKind>,
) -> bool {
    if actor_id.is_empty() {
        return false;
    }
    if let Some(index) = reactions
        .iter()
        .position(|entry| entry.actor_id == actor_id)
    {
        return update_existing_reaction(reactions, index, kind);
    }
    append_reaction(reactions, actor_id, kind)
}

fn update_existing_reaction(
    reactions: &mut Vec<MessageReaction>,
    index: usize,
    kind: Option<ReactionKind>,
) -> bool {
    match kind {
        Some(kind) if reactions[index].kind == kind => false,
        Some(kind) => {
            reactions[index].kind = kind;
            true
        }
        None => {
            reactions.remove(index);
            true
        }
    }
}

fn append_reaction(
    reactions: &mut Vec<MessageReaction>,
    actor_id: &str,
    kind: Option<ReactionKind>,
) -> bool {
    let Some(kind) = kind else {
        return false;
    };
    reactions.push(MessageReaction {
        actor_id: actor_id.to_owned(),
        kind,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::{merge_reactions, set_actor_reaction, MessageReaction, ReactionKind};

    #[test]
    fn actor_has_at_most_one_reaction() {
        let mut reactions = Vec::new();
        assert!(set_actor_reaction(
            &mut reactions,
            "alice",
            Some(ReactionKind::Like)
        ));
        assert!(set_actor_reaction(
            &mut reactions,
            "alice",
            Some(ReactionKind::Love)
        ));
        assert_eq!(
            reactions,
            vec![MessageReaction {
                actor_id: "alice".into(),
                kind: ReactionKind::Love,
            }]
        );
        assert!(set_actor_reaction(&mut reactions, "alice", None));
        assert!(reactions.is_empty());
    }
    #[test]
    fn concurrent_actors_merge_without_loss() {
        let before = vec![MessageReaction {
            actor_id: "alice".into(),
            kind: ReactionKind::Like,
        }];
        let changed = vec![MessageReaction {
            actor_id: "alice".into(),
            kind: ReactionKind::Love,
        }];
        let latest = vec![
            MessageReaction {
                actor_id: "alice".into(),
                kind: ReactionKind::Like,
            },
            MessageReaction {
                actor_id: "bob".into(),
                kind: ReactionKind::Haha,
            },
        ];
        assert_eq!(
            merge_reactions(&before, &changed, &latest),
            vec![
                MessageReaction {
                    actor_id: "alice".into(),
                    kind: ReactionKind::Love
                },
                MessageReaction {
                    actor_id: "bob".into(),
                    kind: ReactionKind::Haha
                },
            ]
        );
    }
}
