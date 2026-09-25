use super::view::DiscoverProps;
use crate::components::avatar::Avatar;
use crate::model::PublicGhostProfile;
use yew::prelude::*;
pub(crate) fn render_lookup_result(
    found: Option<PublicGhostProfile>,
    props: &DiscoverProps,
) -> Html {
    let Some(found) = found else {
        return Html::default();
    };
    let target = public_target(&found);
    let display_name = public_display_name(&found, &target);
    let on_start = props.on_start_chat.clone();
    html! {
        <div class="card public-profile-result">
            <div class="public-profile-title">
                <Avatar label={display_name.clone()} reference={found.avatar.clone()} size={64} />
                <div>
                    <h3>{display_name}<span class="verified-mark">{"✓"}</span></h3>
                    <small>{"Verified public Ghost Talk profile"}</small>
                </div>
                <button
                    class="primary"
                    disabled={props.password.is_empty()}
                    onclick={Callback::from(move |_| on_start.emit(target.clone()))}
                >
                    {"Start chat"}
                </button>
            </div>
            <code class="address-code">{found.kaspa_address}</code>
            {render_description(&found.description)}
            <div class="interest-list">
                {for found.interests.into_iter().map(|value| html! { <span>{value}</span> })}
            </div>
        </div>
    }
}

pub(crate) fn render_description(description: &str) -> Html {
    if description.is_empty() {
        Html::default()
    } else {
        html! { <p>{description.to_string()}</p> }
    }
}

pub(crate) fn public_target(user: &PublicGhostProfile) -> String {
    user.preferred_name()
        .map(str::to_owned)
        .unwrap_or_else(|| user.kaspa_address.clone())
}

pub(crate) fn public_display_name(user: &PublicGhostProfile, target: &str) -> String {
    if !user.username.is_empty() {
        user.username.clone()
    } else if !user.display_name.is_empty() {
        user.display_name.clone()
    } else {
        target.to_string()
    }
}

pub(crate) fn render_public_directory(
    users: Vec<PublicGhostProfile>,
    public_count: usize,
    on_start_chat: Callback<String>,
) -> Html {
    html! {
        <div class="card discover-public-list">
            <div class="public-profile-title">
                <div>
                    <h3>{"Observed public users"}</h3>
                    <small>{"Newest owner-verified public profiles observed by this client."}</small>
                </div>
                <span class="directory-count">{public_count}</span>
            </div>
            {render_public_users(users, on_start_chat)}
        </div>
    }
}

pub(crate) fn render_public_users(
    users: Vec<PublicGhostProfile>,
    on_start_chat: Callback<String>,
) -> Html {
    if users.is_empty() {
        return html! { <p class="muted">{"No public profiles have been observed yet."}</p> };
    }
    let rows = users
        .into_iter()
        .take(100)
        .map(|user| render_public_user(user, on_start_chat.clone()));
    html! { <div class="public-user-list">{for rows}</div> }
}

pub(crate) fn render_public_user(
    user: PublicGhostProfile,
    on_start_chat: Callback<String>,
) -> Html {
    let target = public_target(&user);
    let display_name = public_display_name(&user, &target);
    let target_for_click = target.clone();
    html! {
        <div class="public-user-row">
            <Avatar label={display_name.clone()} reference={user.avatar.clone()} size={42} />
            <div>
                <b>{display_name}<span class="verified-mark">{"✓"}</span></b>
                <small>{target}</small>
            </div>
            <button
                class="primary"
                onclick={Callback::from(move |_| on_start_chat.emit(target_for_click.clone()))}
            >
                {"Start chat"}
            </button>
        </div>
    }
}
