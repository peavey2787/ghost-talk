use super::form::text_input;
use crate::{
    components::recipient_input::{resolve_recipient_target, RecipientInput},
    model::{Contact, Profile, ProfilePatch},
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ContactsProps {
    pub profile: Profile,
    pub password: String,
    pub prefill_target: String,
    pub on_prefill_consumed: Callback<()>,
    pub on_update: Callback<ProfilePatch>,
    pub on_open_chat: Callback<String>,
}

fn add_contact_callback(
    props: &ContactsProps,
    label: UseStateHandle<String>,
    target: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        let Some(destination) = contact_destination(&profile, &password, &target, &status, &busy)
        else {
            return;
        };
        busy.set(true);
        status.set("Resolving Kaspa destination…".into());
        let profile = profile.clone();
        let password = password.clone();
        let display = (*label).clone();
        let status = status.clone();
        let busy = busy.clone();
        let label = label.clone();
        let target = target.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::contact::resolve_and_add(
                &profile,
                &password,
                &destination,
                &display,
            )
            .await
            {
                Ok(patch) => {
                    on_update.emit(patch);
                    label.set(String::new());
                    target.set(String::new());
                    status.set("Contact added.".into());
                }
                Err(error) => status.set(error),
            }
            busy.set(false);
        });
    })
}

fn contact_destination(
    profile: &Profile,
    password: &str,
    target: &UseStateHandle<String>,
    status: &UseStateHandle<String>,
    busy: &UseStateHandle<bool>,
) -> Option<String> {
    let raw = target.trim().to_string();
    if raw.is_empty() || password.is_empty() || **busy {
        return None;
    }
    match resolve_recipient_target(profile, &raw) {
        Ok(value) => Some(value),
        Err(error) => {
            status.set(error);
            None
        }
    }
}

fn contacts_form(
    props: &ContactsProps,
    label: UseStateHandle<String>,
    target: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    add: Callback<MouseEvent>,
) -> Html {
    let target_change = {
        let target = target.clone();
        Callback::from(move |value: String| target.set(value))
    };
    let status_html = if !status.is_empty() {
        html! { <div class="status">{(*status).clone()}</div> }
    } else {
        Default::default()
    };
    html! {
        <form class="card form-grid" onsubmit={Callback::from(|event:web_sys::SubmitEvent| event.prevent_default())}>
            <h3>{"Add contact"}</h3>
            <label>{"Name (optional)"}<input value={(*label).clone()} oninput={text_input(label)} placeholder="Uses verified public name when available" /></label>
            <label>{"Contact, Kaspa address, KNS, or dot.k"}<RecipientInput profile={props.profile.clone()} value={(*target).clone()} on_change={target_change} placeholder="Contact, alice.kas, alice.k, or kaspa:…" /></label>
            <small>{"If no public profile exists, the first message performs a private Kaspa request/accept bootstrap."}</small>
            <button type="submit" class="primary" disabled={target.trim().is_empty() || props.password.is_empty() || *busy} onclick={add}>{if *busy {"Resolving…"} else {"Add contact"}}</button>
            {status_html}
        </form>
    }
}

fn contact_card(contact: &Contact, on_open_chat: &Callback<String>) -> Html {
    let view = crate::view_models::ContactViewModel::from(contact);
    let target = view.target.clone();
    let on_open = on_open_chat.clone();
    let verified = if view.verified_public {
        html! { <span class="verified-mark">{"✓"}</span> }
    } else {
        Default::default()
    };
    html! {
        <div class="card row-card">
            <span class="avatar">{view.label.chars().next().unwrap_or('G').to_ascii_uppercase()}</span>
            <div><b>{view.label.clone()}{verified}</b><small>{view.subtitle.clone()}</small></div>
            <button onclick={Callback::from(move |_| on_open.emit(target.clone()))}>{"Chat"}</button>
        </div>
    }
}

#[hook]
fn use_contact_prefill(prefill: String, consumed: Callback<()>, target: UseStateHandle<String>) {
    use_effect_with(prefill, move |value| {
        if !value.trim().is_empty() {
            target.set((*value).clone());
            consumed.emit(());
        }
        || ()
    });
}

fn contacts_list(props: &ContactsProps) -> Html {
    if props.profile.contacts.is_empty() {
        return html! { <div class="card muted">{"No contacts yet. You can also start a one-off chat directly from Chats."}</div> };
    }
    html! {
        <>{for props.profile.contacts.iter().map(|contact| contact_card(contact, &props.on_open_chat))}</>
    }
}

#[component(ContactsView)]
pub fn contacts_view(props: &ContactsProps) -> Html {
    let label = use_state(String::new);
    let target = use_state(String::new);
    let status = use_state(String::new);
    let busy = use_state(|| false);
    use_contact_prefill(
        props.prefill_target.clone(),
        props.on_prefill_consumed.clone(),
        target.clone(),
    );
    let add = add_contact_callback(
        props,
        label.clone(),
        target.clone(),
        status.clone(),
        busy.clone(),
    );
    html! {
        <section class="page">
            <div class="page-head"><div><h2>{"Contacts"}</h2><p>{"Save people by Kaspa address, KNS, or dot.k. Public discovery is optional."}</p></div></div>
            {contacts_form(props, label, target, status, busy, add)}
            <div class="card-list">{contacts_list(props)}</div>
            <crate::components::kasia::KasiaPanel profile={props.profile.clone()} password={props.password.clone()} on_update={props.on_update.clone()} />
        </section>
    }
}
