use serde::{Deserialize, Serialize};

/// A member's standing in a room. Every member can text and talk; moderators
/// can also manage membership. Any role this build does not know reads as a
/// plain member.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Moderator,
    #[default]
    #[serde(other)]
    Member,
}
