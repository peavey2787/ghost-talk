use crate::{
    components::{authenticated_profile::AuthenticatedProfileProps, form::select_value},
    model::Profile,
};
use yew::prelude::*;

mod conversation;
pub(crate) use conversation::KasiaConversation;

#[component(KasiaPanel)]
pub fn kasia_panel(props: &AuthenticatedProfileProps) -> Html {
    let selected = use_state(|| first_contact_id(&props.profile));
    html! {
        <div class="card form-grid">
            <h3>{"Kasia interoperability"}</h3>
            <small>{"🔒 Kasia is a compatibility mode and is not equivalent to 🛡 Ghost PQ."}</small>
            {contact_selector(props, selected.clone())}
            {render_selected(props, &selected)}
        </div>
    }
}

fn first_contact_id(profile: &Profile) -> String {
    profile
        .contacts
        .first()
        .map(|contact| contact.id.clone())
        .unwrap_or_default()
}

fn contact_selector(props: &AuthenticatedProfileProps, selected: UseStateHandle<String>) -> Html {
    if props.profile.contacts.is_empty() {
        return html! { <small>{"Add a contact before enabling Kasia compatibility."}</small> };
    }
    let on_value = {
        let selected = selected.clone();
        Callback::from(move |value: String| selected.set(value))
    };
    html! {
        <label>{"Contact"}
            <select value={(*selected).clone()} onchange={select_value(on_value)}>
                {for props.profile.contacts.iter().map(|contact| html! {
                    <option value={contact.id.clone()}>{format!("{} · {}", contact.label, contact.kaspa_address())}</option>
                })}
            </select>
        </label>
    }
}

fn render_selected(props: &AuthenticatedProfileProps, selected: &UseStateHandle<String>) -> Html {
    if selected.is_empty() {
        return Html::default();
    }
    html! {
        <KasiaConversation
            profile={props.profile.clone()}
            password={props.password.clone()}
            contact_id={(**selected).clone()}
            on_update={props.on_update.clone()}
        />
    }
}
