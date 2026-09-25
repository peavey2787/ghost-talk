use crate::{
    components::{
        authenticated_profile::AuthenticatedProfileProps, call::RoomVoiceContext, form::status_view,
    },
    model::Profile,
};
use yew::prelude::*;

mod archive;
mod catalog;
mod live;
mod publish;

#[derive(Clone)]
struct StudioState {
    status: UseStateHandle<String>,
    recording: UseStateHandle<Option<ghost_media::MediaReference>>,
    duration_ms: UseStateHandle<u64>,
    catalog: catalog::CatalogUi,
    live: live::LiveUi,
    publish: publish::PublishUi,
}

#[hook]
fn use_studio_state(profile: &Profile) -> StudioState {
    let status = use_state(String::new);
    let recording = use_state(|| None::<ghost_media::MediaReference>);
    let duration_ms = use_state(|| 0u64);
    let creator_id = use_state(|| first_creator(profile));
    StudioState {
        catalog: catalog::CatalogUi {
            creator_name: use_state(|| profile.label.clone()),
            creator_description: use_state(|| profile.settings.public_description.clone()),
            creator_id,
            creator_avatar: use_state(|| None),
            station_name: use_state(String::new),
            station_description: use_state(String::new),
            station_artwork: use_state(|| None),
            station_room_id: use_state(String::new),
            show_title: use_state(String::new),
            show_description: use_state(String::new),
            show_artwork: use_state(|| None),
            show_station_id: use_state(String::new),
        },
        live: live::LiveUi {
            record_local: use_state(|| true),
            rtmp_server: use_state(String::new),
            rtmp_key: use_state(String::new),
            relay_url: use_state(String::new),
            recording: recording.clone(),
            started_at_ms: use_state(|| None::<f64>),
            duration_ms: duration_ms.clone(),
            status: status.clone(),
        },
        publish: publish::PublishUi {
            show_id: use_state(|| first_show(profile)),
            title: use_state(String::new),
            description: use_state(String::new),
            tags: use_state(String::new),
            recording: recording.clone(),
        },
        status,
        recording,
        duration_ms,
    }
}

#[component(StudioView)]
pub fn studio_view(props: &AuthenticatedProfileProps) -> Html {
    let state = use_studio_state(&props.profile);
    let room_voice = use_context::<RoomVoiceContext>();
    html! {
        <section class="page studio-page">
            <div class="page-head"><div><h2>{"Studio"}</h2><p>{"Creator profiles, Room broadcasts, recordings and podcasts share one media pipeline."}</p></div></div>
            {catalog::render(&props.profile, &state.catalog, state.status.clone(), props.on_update.clone())}
            <div class="studio-grid">
                {live::render(&props.profile, &state.live, room_voice)}
                <archive::ArchivePanel profile={props.profile.clone()} password={props.password.clone()} recording={state.recording.clone()} status={state.status.clone()} on_update={props.on_update.clone()} />
                {publish::render(&props.profile, &props.password, state.duration_ms.clone(), &state.publish, state.status.clone(), props.on_update.clone())}
            </div>
            {render_catalog(&props.profile)}
            {status_view(&state.status)}
        </section>
    }
}

fn first_creator(profile: &Profile) -> String {
    profile
        .broadcast_catalog()
        .creators()
        .first()
        .map(|item| item.id.clone())
        .unwrap_or_default()
}

fn first_show(profile: &Profile) -> String {
    profile
        .broadcast_catalog()
        .shows()
        .first()
        .map(|item| item.id.clone())
        .unwrap_or_default()
}

fn render_station(profile: &Profile, station: &ghost_broadcast::StationProfile) -> Html {
    let live_room = station
        .live_room_id
        .as_deref()
        .and_then(|id| profile.rooms.iter().find(|room| room.id == id));
    let live = live_room.map(|_| "🔴 LIVE").unwrap_or("Station");
    let presenters = live_room
        .map(|room| {
            room.members()
                .iter()
                .filter(|member| {
                    matches!(
                        member.role,
                        ghost_rooms::Role::Moderator | ghost_rooms::Role::Presenter
                    )
                })
                .count()
        })
        .unwrap_or(0);
    let detail = if presenters == 0 {
        station.description.clone()
    } else {
        format!(
            "{} · {} presenter(s) from Room roles",
            station.description, presenters
        )
    };
    html! { <div class="row-card"><span><b>{station.name.clone()}</b><small>{detail}</small></span><span>{live}</span></div> }
}

fn render_catalog(profile: &Profile) -> Html {
    let catalog = profile.broadcast_catalog();
    if catalog.stations().is_empty() && catalog.shows().is_empty() && catalog.episodes().is_empty()
    {
        return Html::default();
    }
    html! {
        <div class="card">
            <h3>{"Published studio catalog"}</h3>
            <div class="card-list">
                {for catalog.stations().iter().map(|station| render_station(profile, station))}
                {for catalog.shows().iter().map(|show| html! { <div class="row-card"><span><b>{show.title.clone()}</b><small>{show.description.clone()}</small></span><span>{"Podcast"}</span></div> })}
                {for catalog.episodes().iter().map(|episode| html! { <div class="row-card"><span><b>{episode.title.clone()}</b><small>{format!("{} bytes · signed manifest", episode.media.size)}</small></span><span>{"Episode"}</span></div> })}
            </div>
        </div>
    }
}
