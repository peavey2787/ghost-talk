mod application_router;
pub(crate) use application_router::{ApplicationEvent, ApplicationRouter};
mod mailbox;
pub(crate) use mailbox::process_ready_mailbox;
mod contact_accept;
mod numeric;
mod profile_updates;
pub(crate) use profile_updates::apply_profile_patch;
mod state;
pub(crate) use state::App;
mod startup;
pub(crate) use startup::use_initial_profile_load;
mod profile_runtime;
pub(crate) use profile_runtime::{
    activate_profile_callback, active_profile, apply_directory_event, identity_gate_view,
    loading_view, register_network_listener, start_chat_callback, switch_user_callback,
    update_profile_callback,
};
mod shell;
pub(crate) use shell::{app_shell, mark_chat_read, persist_profiles, upsert_profile};
mod room_ingress;
