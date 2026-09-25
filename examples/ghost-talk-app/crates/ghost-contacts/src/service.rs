use crate::{Contact, ContactProfileUpdate, ContactStore};
use ghost_domain::identity::PeerBinding;

/// Single authenticated contact identity and collection authority.
pub struct ContactService;

impl ContactService {
    pub fn resolve_peer<'a>(contacts: &'a ContactStore, peer: &PeerBinding) -> Option<&'a Contact> {
        contacts.iter().find(|contact| contact.matches_peer(peer))
    }

    pub fn resolve_peer_id(contacts: &ContactStore, peer: &PeerBinding) -> Option<String> {
        Self::resolve_peer(contacts, peer).map(|contact| contact.id.clone())
    }

    pub fn by_id<'a>(contacts: &'a ContactStore, id: &str) -> Option<&'a Contact> {
        contacts.iter().find(|contact| contact.id == id)
    }

    pub fn by_index(contacts: &ContactStore, index: usize) -> Option<&Contact> {
        contacts.get(index)
    }

    pub fn name_conflicts(contacts: &ContactStore, label: &str, except_id: Option<&str>) -> bool {
        let label = label.trim();
        !label.is_empty()
            && contacts.iter().any(|contact| {
                contact.id != except_id.unwrap_or("") && contact.label.eq_ignore_ascii_case(label)
            })
    }

    pub fn unique_label(
        contacts: &ContactStore,
        requested: &str,
        except_id: Option<&str>,
    ) -> String {
        let requested = requested.trim();
        let base = if requested.is_empty() {
            "Contact"
        } else {
            requested
        };
        if !Self::name_conflicts(contacts, base, except_id) {
            return base.to_string();
        }
        (2_u32..)
            .map(|suffix| format!("{base} {suffix}"))
            .find(|candidate| !Self::name_conflicts(contacts, candidate, except_id))
            .expect("an unbounded numeric suffix always yields a unique contact label")
    }

    pub fn new_contact(
        id: String,
        label: String,
        kaspa_address: String,
        hydra_handle: Option<String>,
    ) -> Contact {
        if let Some(hydra_id) = hydra_handle {
            if let Ok(peer) = PeerBinding::new(kaspa_address.clone(), hydra_id) {
                return Contact::authenticated(id, label, &peer);
            }
        }
        Contact::pending(id, label, kaspa_address)
    }

    pub fn push(contacts: &mut ContactStore, contact: Contact) {
        contacts.push_owned(contact);
    }
    pub fn remove_by_id(contacts: &mut ContactStore, id: &str) -> bool {
        let before = contacts.len();
        contacts.retain_owned(|contact| contact.id != id);
        contacts.len() != before
    }

    pub fn bind_peer_by_id(contacts: &mut ContactStore, id: &str, peer: &PeerBinding) -> bool {
        let Some(contact) = contact_by_id_mut(contacts, id) else {
            return false;
        };
        contact.set_peer_binding(peer);
        true
    }

    pub fn bind_peer(contacts: &mut ContactStore, peer: &PeerBinding) -> bool {
        let Some(contact) = contacts
            .as_mut_slice()
            .iter_mut()
            .find(|contact| contact.matches_peer(peer))
        else {
            return false;
        };
        contact.set_peer_binding(peer);
        true
    }

    pub fn apply_public_profile_by_id(
        contacts: &mut ContactStore,
        id: &str,
        update: ContactProfileUpdate,
    ) -> bool {
        let Some(contact) = contact_by_id_mut(contacts, id) else {
            return false;
        };
        contact.apply_public_profile(update);
        true
    }

    pub fn restore_missing_identity_by_id(
        contacts: &mut ContactStore,
        id: &str,
        label: String,
        hydra_handle: Option<String>,
    ) -> bool {
        let Some(contact) = contact_by_id_mut(contacts, id) else {
            return false;
        };
        contact.restore_missing_identity(label, hydra_handle);
        true
    }

    pub fn replace_at_index(contacts: &mut ContactStore, index: usize, contact: Contact) -> bool {
        let Some(slot) = contacts.as_mut_slice().get_mut(index) else {
            return false;
        };
        *slot = contact;
        true
    }

    pub fn rename(contacts: &mut ContactStore, id: &str, label: String) -> bool {
        let Some(contact) = contact_by_id_mut(contacts, id) else {
            return false;
        };
        contact.label = label;
        true
    }

    pub fn mark_verified(contacts: &mut ContactStore, id: &str, verified: bool) -> bool {
        let Some(contact) = contact_by_id_mut(contacts, id) else {
            return false;
        };
        contact.verified_public = verified;
        true
    }

    /// Reconcile one async contact upsert against the current authoritative store.
    /// Only fields changed relative to `before` may replace concurrent local edits.
    pub fn reconcile_upsert(contacts: &mut ContactStore, before: Option<&Contact>, after: Contact) {
        if let Some(current) = contacts
            .as_mut_slice()
            .iter_mut()
            .find(|contact| contact.id == after.id)
        {
            reconcile_existing_contact(current, before, after);
            return;
        }
        if before.is_none() {
            contacts.push_owned(after);
        }
    }

    pub fn remove_if_unchanged(contacts: &mut ContactStore, before: &Contact) {
        let unchanged = contacts
            .iter()
            .any(|current| current.id == before.id && current == before);
        if unchanged {
            contacts.retain_owned(|contact| contact.id != before.id);
        }
    }
}

fn reconcile_existing_contact(current: &mut Contact, before: Option<&Contact>, after: Contact) {
    if let Some(before) = before {
        merge_changed_fields(before, &after, current);
    } else if current == &after || same_authenticated_peer(current, &after) {
        *current = after;
    }
}

fn contact_by_id_mut<'a>(contacts: &'a mut ContactStore, id: &str) -> Option<&'a mut Contact> {
    contacts
        .as_mut_slice()
        .iter_mut()
        .find(|contact| contact.id == id)
}

fn same_authenticated_peer(left: &Contact, right: &Contact) -> bool {
    match (left.peer_binding(), right.peer_binding()) {
        (Some(left), Some(right)) => left.same_peer(&right),
        _ => false,
    }
}

fn merge_changed_fields(before: &Contact, after: &Contact, latest: &mut Contact) {
    latest.reconcile_identity_fields(before, after);
    merge_names(before, after, latest);
    merge_public_state(before, after, latest);
}

fn merge_names(before: &Contact, after: &Contact, latest: &mut Contact) {
    if before.label != after.label {
        latest.label = after.label.clone();
    }
    if before.kns_name != after.kns_name {
        latest.kns_name = after.kns_name.clone();
    }
    if before.dotk_name != after.dotk_name {
        latest.dotk_name = after.dotk_name.clone();
    }
}

fn merge_public_state(before: &Contact, after: &Contact, latest: &mut Contact) {
    if before.verified_public != after.verified_public {
        latest.verified_public = after.verified_public;
    }
    if before.public_username != after.public_username {
        latest.public_username = after.public_username.clone();
    }
    if before.blocked != after.blocked {
        latest.blocked = after.blocked;
    }
}

#[cfg(test)]
mod tests {
    use super::ContactService;
    use crate::{policy::should_deliver, AnonymousPolicy, Contact, ContactStore};
    use ghost_domain::identity::PeerBinding;

    fn contact() -> Contact {
        Contact::authenticated(
            "contact".into(),
            "Peer".into(),
            &PeerBinding::new("kaspatest:peer", "hydra-peer").unwrap(),
        )
    }

    #[test]
    fn exact_resolution_requires_both_authenticated_identifiers() {
        let contacts = ContactStore::from(vec![contact()]);
        let exact = PeerBinding::new("KASPATEST:PEER", "hydra-peer").unwrap();
        let wrong_hydra = PeerBinding::new("kaspatest:peer", "other").unwrap();
        assert_eq!(
            ContactService::resolve_peer_id(&contacts, &exact).as_deref(),
            Some("contact")
        );
        assert!(ContactService::resolve_peer(&contacts, &wrong_hydra).is_none());
    }

    #[test]
    fn incomplete_historical_contact_is_not_authenticated() {
        let mut incomplete = contact();
        incomplete.clear_hydra_binding_for_test();
        let peer = PeerBinding::new("kaspatest:peer", "hydra-peer").unwrap();
        assert!(
            ContactService::resolve_peer(&ContactStore::from(vec![incomplete]), &peer).is_none()
        );
    }

    #[test]
    fn ignored_unknown_is_not_delivered() {
        assert!(!should_deliver(None, AnonymousPolicy::Ignore, false));
        assert!(should_deliver(None, AnonymousPolicy::RequestsOnly, true));
    }
}
