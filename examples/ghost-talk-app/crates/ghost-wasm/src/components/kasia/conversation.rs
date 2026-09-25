use crate::{
    components::form::{status_view, text_input},
    model::{Profile, ProfilePatch},
};
use ghost_kasia::KasiaMessage;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct KasiaConversationProps {
    pub profile: Profile,
    pub password: String,
    pub contact_id: String,
    pub on_update: Callback<ProfilePatch>,
}

#[component(KasiaConversation)]
pub fn kasia_conversation(props: &KasiaConversationProps) -> Html {
    let indexer_url = use_state(String::new);
    let message = use_state(String::new);
    let status = use_state(String::new);
    let history = use_state(Vec::<KasiaMessage>::new);
    let mapping = props
        .profile
        .kasia_contacts()
        .by_contact(&props.contact_id)
        .cloned();
    html! {
        <div class="card form-grid kasia-conversation">
            <h4>{"🔒 Kasia compatibility"}</h4>
            <small>{"Explicit compatibility mode. The native composer remains 🛡 Ghost PQ; failures never switch protocols automatically."}</small>
            {protocol_preference(&props.profile, &props.contact_id)}
            {route_controls(props, mapping.clone(), status.clone())}
            <label>{"Kasia Indexer URL"}<input value={(*indexer_url).clone()} oninput={text_input(indexer_url.clone())} placeholder="https://…" /></label>
            {message_controls(props, mapping, indexer_url, message, status.clone(), history.clone())}
            {history_view(&history)}
            {status_view(&status)}
        </div>
    }
}

fn protocol_preference(profile: &Profile, contact_id: &str) -> Html {
    use ghost_api::ConversationMode;
    let availability = crate::controllers::kasia::protocol_availability(profile, contact_id);
    let preferred = crate::controllers::kasia::preferred_new_mode(profile, contact_id);
    let label = match preferred {
        Ok(ConversationMode::GhostPq) => "🛡 Ghost PQ preferred for new conversations",
        Ok(ConversationMode::Kasia) => "🔒 Kasia available for new conversations",
        Err(_) => "No advertised compatible messaging protocol",
    };
    let ghost_pq = if availability.ghost_pq {
        "available"
    } else {
        "unavailable"
    };
    let kasia = if availability.kasia {
        "available"
    } else {
        "unavailable"
    };
    let detail = format!("Ghost PQ: {ghost_pq} · Kasia: {kasia}");
    html! { <div class="row-card"><span><b>{label}</b><small>{detail}</small></span></div> }
}

fn route_controls(
    props: &KasiaConversationProps,
    mapping: Option<ghost_kasia::KasiaContactMapping>,
    status: UseStateHandle<String>,
) -> Html {
    let Some(mapping) = mapping else {
        let profile = props.profile.clone();
        let contact_id = props.contact_id.clone();
        let on_update = props.on_update.clone();
        return html! { <button onclick={Callback::from(move |_| {
            match crate::controllers::kasia::pending_mapping(&profile, &contact_id) {
                Ok(patch) => on_update.emit(patch),
                Err(error) => status.set(error),
            }
        })}>{"Enable Kasia for this contact"}</button> };
    };
    let profile = props.profile.clone();
    let password = props.password.clone();
    let route = mapping.clone();
    let status_send = status.clone();
    let send = Callback::from(move |_| {
        let profile = profile.clone();
        let password = password.clone();
        let route = route.clone();
        let status = status_send.clone();
        spawn_local(async move {
            match crate::controllers::kasia::send_handshake(&profile, &password, &route, false)
                .await
            {
                Ok(result) => {
                    status.set(format!("Kasia handshake sent · {}", result.transaction_id))
                }
                Err(error) => status.set(error),
            }
        });
    });
    html! { <div class="row-card"><span><b>{if mapping.established {"Kasia route established"} else {"Kasia route pending"}}</b><small>{format!("our alias {} · conversation {}", mapping.our_alias, mapping.conversation_id)}</small></span><button onclick={send}>{"Send handshake"}</button></div> }
}

fn message_controls(
    props: &KasiaConversationProps,
    mapping: Option<ghost_kasia::KasiaContactMapping>,
    indexer_url: UseStateHandle<String>,
    message: UseStateHandle<String>,
    status: UseStateHandle<String>,
    history: UseStateHandle<Vec<KasiaMessage>>,
) -> Html {
    let Some(mapping) = mapping else {
        return Html::default();
    };
    let sync = sync_callback(
        props,
        mapping.clone(),
        indexer_url.clone(),
        status.clone(),
        history.clone(),
    );
    let send = send_message_callback(props, mapping.clone(), message.clone(), status.clone());
    html! { <><div class="button-row"><button onclick={sync}>{"Sync Kasia"}</button></div><label>{"Kasia message"}<input value={(*message).clone()} oninput={text_input(message.clone())} placeholder="Encrypted Kasia text" /></label><button disabled={!mapping.established || message.trim().is_empty()} onclick={send}>{"Send via Kasia"}</button></> }
}

fn sync_callback(
    props: &KasiaConversationProps,
    mapping: ghost_kasia::KasiaContactMapping,
    indexer_url: UseStateHandle<String>,
    status: UseStateHandle<String>,
    history: UseStateHandle<Vec<KasiaMessage>>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        let url = indexer_url.trim().to_string();
        if url.is_empty() {
            status.set("Enter the Kasia Indexer URL first.".into());
            return;
        }
        let profile = profile.clone();
        let password = password.clone();
        let mapping = mapping.clone();
        let status = status.clone();
        let history = history.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::kasia::sync(&profile, &password, &mapping, &url, 0).await {
                Ok(result) => {
                    if let Some(patch) = result.patch {
                        on_update.emit(patch);
                    }
                    history.set(result.messages);
                    status.set("Kasia sync complete.".into());
                }
                Err(error) => status.set(error),
            }
        });
    })
}

fn send_message_callback(
    props: &KasiaConversationProps,
    mapping: ghost_kasia::KasiaContactMapping,
    message: UseStateHandle<String>,
    status: UseStateHandle<String>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    Callback::from(move |_| {
        let text = message.trim().to_string();
        if text.is_empty() {
            return;
        }
        let profile = profile.clone();
        let password = password.clone();
        let mapping = mapping.clone();
        let message = message.clone();
        let status = status.clone();
        spawn_local(async move {
            match crate::controllers::kasia::send_message(&profile, &password, &mapping, &text)
                .await
            {
                Ok(result) => {
                    message.set(String::new());
                    status.set(format!("Kasia message sent · {}", result.transaction_id));
                }
                Err(error) => status.set(error),
            }
        });
    })
}

fn history_view(history: &UseStateHandle<Vec<KasiaMessage>>) -> Html {
    if history.is_empty() {
        return Html::default();
    }
    html! { <div class="card-list">{for history.iter().map(|message| html! { <div class="row-card"><span><b>{"🔒 Kasia"}</b><small>{message.text.clone()}</small></span><small>{message.tx_id.clone()}</small></div> })}</div> }
}
