use crate::Contact;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnonymousPolicy {
    Allow,
    RequestsOnly,
    Ignore,
}

pub fn should_deliver(contact: Option<&Contact>, policy: AnonymousPolicy, is_invite: bool) -> bool {
    match contact {
        Some(contact) => !contact.blocked,
        None => match policy {
            AnonymousPolicy::Allow => true,
            AnonymousPolicy::RequestsOnly => is_invite,
            AnonymousPolicy::Ignore => false,
        },
    }
}
