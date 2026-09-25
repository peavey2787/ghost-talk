use crate::model::{Profile, ProfilePatch};
use ghost_broadcast::{CreatorProfile, PodcastEpisode, PodcastShow, StationProfile};
use ghost_media::MediaReference;

#[derive(Clone)]
pub(crate) struct EpisodePublishRequest {
    pub(crate) show_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) tags: String,
    pub(crate) duration_ms: u64,
    pub(crate) media: MediaReference,
}

pub(crate) fn upsert_creator(
    profile: &Profile,
    display_name: &str,
    description: &str,
    avatar: Option<MediaReference>,
) -> Result<ProfilePatch, String> {
    let owner = primary_address(profile)?;
    let existing = profile
        .broadcast_catalog()
        .creators()
        .iter()
        .find(|creator| creator.owner_kaspa_address == owner);
    let value = CreatorProfile {
        id: existing
            .map(|item| item.id.clone())
            .unwrap_or(crate::random_id()?),
        display_name: required(display_name, "Creator display name")?,
        owner_kaspa_address: owner,
        verified_name: verified_name(profile),
        avatar: avatar.or_else(|| existing.and_then(|item| item.avatar.clone())),
        description: description.trim().to_owned(),
    };
    let mut patch = ProfilePatch::new(&profile.id);
    patch.creator(value);
    Ok(patch)
}

pub(crate) fn upsert_station(
    profile: &Profile,
    creator_id: &str,
    name: &str,
    description: &str,
    artwork: Option<MediaReference>,
    live_room_id: Option<&str>,
) -> Result<ProfilePatch, String> {
    ensure_creator(profile, creator_id)?;
    let existing = profile
        .broadcast_catalog()
        .stations()
        .iter()
        .find(|station| {
            station.owner == creator_id && station.name.eq_ignore_ascii_case(name.trim())
        });
    let value = StationProfile {
        id: existing
            .map(|item| item.id.clone())
            .unwrap_or(crate::random_id()?),
        name: required(name, "Station name")?,
        artwork: artwork.or_else(|| existing.and_then(|item| item.artwork.clone())),
        description: description.trim().to_owned(),
        owner: creator_id.to_owned(),
        shows: existing.map(|item| item.shows.clone()).unwrap_or_default(),
        live_room_id: validated_broadcast_room(profile, live_room_id)?,
    };
    let mut patch = ProfilePatch::new(&profile.id);
    patch.station(value);
    Ok(patch)
}

pub(crate) fn upsert_show(
    profile: &Profile,
    creator_id: &str,
    station_id: &str,
    title: &str,
    description: &str,
    artwork: Option<MediaReference>,
) -> Result<ProfilePatch, String> {
    ensure_creator(profile, creator_id)?;
    let existing = profile
        .broadcast_catalog()
        .shows()
        .iter()
        .find(|show| show.owner == creator_id && show.title.eq_ignore_ascii_case(title.trim()));
    let value = PodcastShow {
        id: existing
            .map(|item| item.id.clone())
            .unwrap_or(crate::random_id()?),
        title: required(title, "Show title")?,
        description: description.trim().to_owned(),
        artwork: artwork.or_else(|| existing.and_then(|item| item.artwork.clone())),
        owner: creator_id.to_owned(),
    };
    let mut station = matching_station(profile, creator_id, station_id)?.clone();
    if !station.shows.iter().any(|id| id == &value.id) {
        station.shows.push(value.id.clone());
    }
    let mut patch = ProfilePatch::new(&profile.id);
    patch.show(value);
    patch.station(station);
    Ok(patch)
}

pub(crate) async fn publish_episode(
    profile: &Profile,
    password: &str,
    request: EpisodePublishRequest,
) -> Result<ProfilePatch, String> {
    if profile.broadcast_catalog().show(&request.show_id).is_none() {
        return Err("Select a podcast show before publishing.".into());
    }
    validate_media(&request.media)?;
    let title = required(&request.title, "Episode title")?;
    let manifest = crate::controllers::media::sign_recording_manifest(
        profile,
        password,
        &request.media,
        &title,
        request.duration_ms,
    )
    .await?;
    let value = PodcastEpisode {
        id: crate::random_id()?,
        show_id: request.show_id,
        title,
        description: request.description.trim().to_owned(),
        media: request.media,
        manifest: Some(manifest),
        tags: parse_tags(&request.tags),
    };
    let mut patch = ProfilePatch::new(&profile.id);
    patch.episode(value);
    Ok(patch)
}

fn matching_station<'a>(
    profile: &'a Profile,
    creator_id: &str,
    station_id: &str,
) -> Result<&'a StationProfile, String> {
    let station = profile
        .broadcast_catalog()
        .station(station_id)
        .ok_or_else(|| "Select a station for this show.".to_string())?;
    (station.owner == creator_id)
        .then_some(station)
        .ok_or_else(|| "The selected station belongs to another creator.".into())
}

fn validated_broadcast_room(
    profile: &Profile,
    room_id: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(room_id) = room_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let room = profile
        .rooms
        .iter()
        .find(|room| room.id == room_id)
        .ok_or_else(|| "Selected live Room is unavailable.".to_string())?;
    room.broadcast_enabled()
        .then(|| Some(room.id.clone()))
        .ok_or_else(|| "Selected Room is not configured for broadcasting.".into())
}

fn primary_address(profile: &Profile) -> Result<String, String> {
    profile
        .wallet
        .as_ref()
        .and_then(|wallet| wallet.public.receive_addresses.first())
        .filter(|address| !address.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            "Create or import the Kaspa wallet before creating a creator profile.".into()
        })
}

fn ensure_creator(profile: &Profile, creator_id: &str) -> Result<(), String> {
    profile
        .broadcast_catalog()
        .creator(creator_id)
        .map(|_| ())
        .ok_or_else(|| "Select a creator profile first.".into())
}

fn verified_name(profile: &Profile) -> Option<String> {
    let address = primary_address(profile).ok()?;
    profile
        .public_directory
        .iter()
        .find(|entry| entry.kaspa_address.eq_ignore_ascii_case(&address))
        .and_then(|entry| entry.verified_primary_name())
        .map(str::to_owned)
}

fn required(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required."))
    } else if value.len() > 160 {
        Err(format!("{label} is too long."))
    } else {
        Ok(value.to_owned())
    }
}

fn parse_tags(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .take(16)
        .map(str::to_owned)
        .collect()
}

fn validate_media(media: &MediaReference) -> Result<(), String> {
    if media.media_id.len() != 64 || media.size == 0 {
        return Err("Recording reference is incomplete.".into());
    }
    if !media.content_type.starts_with("audio/") {
        return Err("Podcast episodes require an audio recording.".into());
    }
    Ok(())
}
