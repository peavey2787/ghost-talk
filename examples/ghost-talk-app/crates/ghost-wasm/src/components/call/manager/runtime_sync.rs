use crate::model::{Profile, ProfilePatch, WalletProjection};
use ghost_domain::call::CallManager;
use std::{cell::RefCell, rc::Rc};
use yew::prelude::*;

pub(super) fn reset_call_owner(
    manager: &Rc<RefCell<CallManager>>,
    owner: &Rc<RefCell<String>>,
    profile_id: &str,
) {
    if *owner.borrow() != profile_id {
        manager.borrow_mut().reset();
        *owner.borrow_mut() = profile_id.to_owned();
    }
}

pub(super) fn sync_profile_ref(profile_ref: &Rc<RefCell<Profile>>, incoming: &Profile) {
    let replace = {
        let current = profile_ref.borrow();
        current.id != incoming.id || incoming.state_revision() > current.state_revision()
    };
    if replace {
        *profile_ref.borrow_mut() = incoming.clone();
    }
}

pub(super) fn realtime_wallet_progress_callback(
    profile_ref: Rc<RefCell<Profile>>,
    render_epoch: UseStateHandle<u64>,
    on_update: Callback<ProfilePatch>,
) -> Callback<WalletProjection> {
    Callback::from(move |public| {
        let before = profile_ref.borrow().clone();
        let Some((profile, patch)) =
            crate::controllers::account::apply_wallet_progress(&before, public)
        else {
            return;
        };
        *profile_ref.borrow_mut() = profile;
        render_epoch.set((*render_epoch).wrapping_add(1));
        on_update.emit(patch);
    })
}
