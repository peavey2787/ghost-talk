use super::{
    prevent_submit, resolve_recipient_target, spawn_local, DiscoverProps, Profile,
    PublicGhostProfile, RecipientInput,
};
use yew::prelude::*;

pub(crate) fn render_lookup_form(
    props: &DiscoverProps,
    query: UseStateHandle<String>,
    result: UseStateHandle<Option<PublicGhostProfile>>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Html {
    let lookup = lookup_callback(props, query.clone(), result, status, busy.clone());
    html! {
        <form class="card form-grid discover-search-card" onsubmit={prevent_submit()}>
            <h3>{"Find a public user"}</h3>
            <label>
                {"Kaspa address, KNS, or dot.k"}
                <RecipientInput
                    profile={props.profile.clone()}
                    value={(*query).clone()}
                    on_change={{let query=query.clone(); Callback::from(move |value:String| query.set(value))}}
                    placeholder="Contact, alice.kas, alice.k, or kaspa:…"
                />
            </label>
            <button
                type="submit"
                class="primary"
                disabled={query.trim().is_empty() || *busy}
                onclick={lookup}
            >
                {if *busy { "Searching…" } else { "Find latest profile" }}
            </button>
        </form>
    }
}

pub(crate) fn lookup_callback(
    props: &DiscoverProps,
    query: UseStateHandle<String>,
    result: UseStateHandle<Option<PublicGhostProfile>>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    Callback::from(move |_| {
        let raw_target = query.trim().to_string();
        if raw_target.is_empty() || *busy {
            return;
        }
        let target = match resolve_recipient_target(&profile, &raw_target) {
            Ok(value) => value,
            Err(error) => {
                status.set(error);
                return;
            }
        };
        busy.set(true);
        status.set("Checking the latest owner-signed Ghost Talk profile…".into());
        lookup_profile_async(
            profile.clone(),
            target,
            result.clone(),
            status.clone(),
            busy.clone(),
        );
    })
}

pub(crate) fn lookup_profile_async(
    profile: Profile,
    target: String,
    result: UseStateHandle<Option<PublicGhostProfile>>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) {
    spawn_local(async move {
        match crate::controllers::discover::lookup_public_profile(&profile, &target).await {
            Ok(found) => {
                status.set(lookup_status(found.is_some()).into());
                result.set(found);
            }
            Err(error) => {
                status.set(error);
                result.set(None);
            }
        }
        busy.set(false);
    });
}

pub(crate) fn lookup_status(found: bool) -> &'static str {
    if found {
        "Latest public profile verified."
    } else {
        "No current public profile was found. Private chat is still available."
    }
}
