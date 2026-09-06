#![forbid(unsafe_code)]

use ghost_core::ContactId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Relationship {
    CloseFriend,
    Friend,
    Acquaintance,
    Anonymous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnonymousPolicy {
    Allow,
    RequestsOnly,
    Ignore,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contact {
    pub id: ContactId,
    pub friendly_name: String,
    pub kns: Option<String>,
    pub kaspa_address: String,
    pub hydra_fingerprint: String,
    pub relationship: Relationship,
    pub blocked: bool,
    pub verified: bool,
}

impl Contact {
    pub fn searchable(&self, q: &str) -> bool {
        let q = q.to_ascii_lowercase();
        self.friendly_name.to_ascii_lowercase().contains(&q)
            || self
                .kns
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(&q)
            || self.kaspa_address.to_ascii_lowercase().contains(&q)
            || self.hydra_fingerprint.to_ascii_lowercase().contains(&q)
    }
}

pub fn should_deliver(c: Option<&Contact>, policy: AnonymousPolicy, is_invite: bool) -> bool {
    match c {
        Some(c) => !c.blocked,
        None => match policy {
            AnonymousPolicy::Allow => true,
            AnonymousPolicy::RequestsOnly => is_invite,
            AnonymousPolicy::Ignore => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_unknown_is_not_delivered() {
        assert!(!should_deliver(None, AnonymousPolicy::Ignore, false));
        assert!(should_deliver(None, AnonymousPolicy::RequestsOnly, true))
    }
}
