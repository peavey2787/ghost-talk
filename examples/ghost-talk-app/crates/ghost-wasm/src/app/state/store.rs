use crate::model::{CallRuntimeEvent, Profile, RealtimeControl, WalletSnapshot};
use std::{cell::RefCell, collections::HashSet, rc::Rc};
use yew::prelude::*;

/// Reactive view projections. Mutable application truth lives in `AppRuntime`;
/// these handles exist only to trigger/render Yew updates and are written by
/// `AppRuntime` methods for fields that mirror runtime-owned data.
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) profiles: UseStateHandle<Vec<Profile>>,
    pub(crate) loaded: UseStateHandle<bool>,
    pub(crate) active_id: UseStateHandle<Option<String>>,
    pub(crate) session_password: UseStateHandle<String>,
    pub(crate) tab: UseStateHandle<String>,
    pub(crate) selected_chat_id: UseStateHandle<String>,
    pub(crate) contact_prefill: UseStateHandle<String>,
    pub(crate) snapshot: UseStateHandle<Option<WalletSnapshot>>,
    pub(crate) network_status: UseStateHandle<String>,
    pub(crate) reconnect_attempts: UseStateHandle<u32>,
    pub(crate) app_status: UseStateHandle<String>,
    pub(crate) realtime_controls: UseStateHandle<Vec<RealtimeControl>>,
    pub(crate) call_events: UseStateHandle<Vec<CallRuntimeEvent>>,
    pub(crate) debug_open: UseStateHandle<bool>,
}

/// Sole mutable owner for application data that must remain current across
/// asynchronous callbacks. Yew state above is a one-way render projection.
#[derive(Clone)]
pub(crate) struct AppRuntime {
    pub(crate) profiles: Rc<RefCell<Vec<Profile>>>,
    pub(crate) password: Rc<RefCell<String>>,
    pub(crate) active_id: Rc<RefCell<Option<String>>>,
    pub(crate) tab: Rc<RefCell<String>>,
    pub(crate) selected_chat_id: Rc<RefCell<String>>,
    pub(crate) realtime: Rc<RefCell<Vec<RealtimeControl>>>,
    pub(crate) call_events: Rc<RefCell<Vec<CallRuntimeEvent>>>,
    pub(crate) mailbox_busy: Rc<RefCell<HashSet<String>>>,
    pub(crate) mailbox_dirty: Rc<RefCell<HashSet<String>>>,
    pub(crate) resume_busy: Rc<RefCell<HashSet<String>>>,
}

#[hook]
pub(crate) fn use_app_state() -> AppState {
    AppState {
        profiles: use_state(Vec::<Profile>::new),
        loaded: use_state(|| false),
        active_id: use_state(|| None::<String>),
        session_password: use_state(String::new),
        tab: use_state(|| "Chats".to_string()),
        selected_chat_id: use_state(String::new),
        contact_prefill: use_state(String::new),
        snapshot: use_state(|| None::<WalletSnapshot>),
        network_status: use_state(|| "disconnected".to_string()),
        reconnect_attempts: use_state(|| 0u32),
        app_status: use_state(String::new),
        realtime_controls: use_state(Vec::<RealtimeControl>::new),
        call_events: use_state(Vec::<CallRuntimeEvent>::new),
        debug_open: use_state(|| false),
    }
}

#[hook]
pub(crate) fn use_app_runtime() -> AppRuntime {
    AppRuntime {
        profiles: use_mut_ref(Vec::<Profile>::new),
        password: use_mut_ref(String::new),
        active_id: use_mut_ref(|| None::<String>),
        tab: use_mut_ref(|| "Chats".to_string()),
        selected_chat_id: use_mut_ref(String::new),
        realtime: use_mut_ref(Vec::<RealtimeControl>::new),
        call_events: use_mut_ref(Vec::<CallRuntimeEvent>::new),
        mailbox_busy: use_mut_ref(HashSet::<String>::new),
        mailbox_dirty: use_mut_ref(HashSet::<String>::new),
        resume_busy: use_mut_ref(HashSet::<String>::new),
    }
}

impl AppRuntime {
    pub(crate) fn replace_profiles(&self, state: &AppState, profiles: Vec<Profile>) {
        *self.profiles.borrow_mut() = profiles.clone();
        state.profiles.set(profiles);
    }

    pub(crate) fn activate(&self, state: &AppState, profile_id: String, password: String) {
        *self.active_id.borrow_mut() = Some(profile_id.clone());
        *self.password.borrow_mut() = password.clone();
        state.active_id.set(Some(profile_id));
        state.session_password.set(password);
    }

    pub(crate) fn clear_session(&self, state: &AppState) {
        *self.active_id.borrow_mut() = None;
        self.password.borrow_mut().clear();
        state.active_id.set(None);
        state.session_password.set(String::new());
    }

    pub(crate) fn select_tab(&self, state: &AppState, tab: String) {
        *self.tab.borrow_mut() = tab.clone();
        state.tab.set(tab);
    }

    pub(crate) fn select_chat(&self, state: &AppState, chat_id: String) {
        *self.selected_chat_id.borrow_mut() = chat_id.clone();
        state.selected_chat_id.set(chat_id);
    }

    pub(crate) fn replace_realtime(&self, state: &AppState, controls: Vec<RealtimeControl>) {
        *self.realtime.borrow_mut() = controls.clone();
        state.realtime_controls.set(controls);
    }

    pub(crate) fn replace_call_events(&self, state: &AppState, events: Vec<CallRuntimeEvent>) {
        *self.call_events.borrow_mut() = events.clone();
        state.call_events.set(events);
    }
}
