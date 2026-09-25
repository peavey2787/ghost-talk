use super::state::{
    now_ms, HtmlSelectElement, Profile, Room, RoomActions, RoomBan, RoomMember, RoomsUi,
};
use super::{input, prevent_submit};
use yew::prelude::*;

pub(crate) fn render_ban_editor(
    ui: &RoomsUi,
    actions: &RoomActions,
    room: &Room,
    is_owner: bool,
) -> Html {
    if !is_owner || ui.ban_target.is_empty() {
        return Html::default();
    }
    let target_label = room
        .members()
        .iter()
        .find(|member| {
            member
                .kaspa_address
                .eq_ignore_ascii_case(ui.ban_target.as_str())
        })
        .map(|member| member.label.clone())
        .unwrap_or_else(|| (*ui.ban_target).clone());
    let ban_unit = ui.ban_unit.clone();
    let cancel_target = ui.ban_target.clone();
    html! {
        <form class="room-ban-editor" onsubmit={prevent_submit()}>
            <b>{format!("Ban {target_label}")}</b>
            <input type="number" min="1" inputmode="numeric" value={(*ui.ban_amount).clone()} oninput={input(ui.ban_amount.clone())} disabled={*ui.ban_unit == "forever"}/>
            <select value={(*ui.ban_unit).clone()} onchange={Callback::from(move |event: Event| ban_unit.set(event.target_unchecked_into::<HtmlSelectElement>().value()))}>
                <option value="minutes">{"Minutes"}</option>
                <option value="hours">{"Hours"}</option>
                <option value="days">{"Days"}</option>
                <option value="weeks">{"Weeks"}</option>
                <option value="forever">{"Forever"}</option>
            </select>
            <button type="submit" class="danger-link" disabled={*ui.busy} onclick={actions.confirm_ban.clone()}>{"Confirm ban"}</button>
            <button type="button" onclick={Callback::from(move |_| cancel_target.set(String::new()))}>{"Cancel"}</button>
        </form>
    }
}

pub(crate) fn render_active_bans(
    ui: &RoomsUi,
    actions: &RoomActions,
    room: &Room,
    is_owner: bool,
) -> Html {
    if !is_owner {
        return Html::default();
    }
    let bans = room
        .bans()
        .iter()
        .filter(|ban| ban.is_active(now_ms()))
        .cloned()
        .collect::<Vec<_>>();
    if bans.is_empty() {
        return Html::default();
    }
    html! {
        <div class="room-ban-list">
            <h3>{"Bans"}</h3>
            {for bans.iter().map(|ban| render_ban_row(ui, actions, ban))}
        </div>
    }
}

pub(crate) fn render_ban_row(ui: &RoomsUi, actions: &RoomActions, ban: &RoomBan) -> Html {
    let address = ban.kaspa_address.clone();
    let unban = actions.unban_member.clone();
    html! {
        <div>
            <span><b>{ban.label.clone()}</b><small>{format!("{}{}", ban.kaspa_address, format_ban_expiry(ban.expires_at))}</small></span>
            <button type="button" disabled={*ui.busy} onclick={Callback::from(move |_| unban.emit(address.clone()))}>{"Unban"}</button>
        </div>
    }
}

pub(crate) fn render_room_status(ui: &RoomsUi) -> Html {
    if ui.status.is_empty() {
        Html::default()
    } else {
        html! { <div class="status floating-status">{(*ui.status).clone()}</div> }
    }
}

pub(crate) fn room_member_is_local(profile: &Profile, member: &RoomMember) -> bool {
    if member
        .hydra_handle
        .as_deref()
        .is_some_and(|handle| profile.hydra_identity_id.as_deref() == Some(handle))
    {
        return true;
    }
    profile.wallet.as_ref().is_some_and(|wallet| {
        wallet
            .public
            .receive_addresses
            .iter()
            .chain(wallet.public.change_addresses.iter())
            .any(|address| address.eq_ignore_ascii_case(&member.kaspa_address))
    })
}

pub(crate) fn parse_ban_expiry(amount: &str, unit: &str) -> Result<Option<f64>, String> {
    if unit == "forever" {
        return Ok(None);
    }
    let amount = parse_ban_amount(amount)?;
    let minutes = amount
        .checked_mul(ban_unit_minutes(unit)?)
        .ok_or_else(|| "Ban duration is too large.".to_string())?;
    let millis = minutes
        .checked_mul(60_000)
        .ok_or_else(|| "Ban duration is too large.".to_string())?;
    Ok(Some(now_ms() + millis as f64))
}

fn parse_ban_amount(amount: &str) -> Result<u64, String> {
    let amount = amount
        .trim()
        .parse::<u64>()
        .map_err(|_| "Ban duration must be a whole number of at least 1.".to_string())?;
    if amount == 0 {
        Err("Ban duration must be at least 1 minute.".into())
    } else {
        Ok(amount)
    }
}

fn ban_unit_minutes(unit: &str) -> Result<u64, String> {
    match unit {
        "minutes" => Ok(1),
        "hours" => Ok(60),
        "days" => Ok(60 * 24),
        "weeks" => Ok(60 * 24 * 7),
        _ => Err("Unknown ban duration unit.".into()),
    }
}

pub(crate) fn format_ban_expiry(expires_at: Option<f64>) -> String {
    match expires_at {
        None => " permanently".into(),
        Some(value) => {
            let remaining_ms = (value - now_ms()).max(0.0);
            let remaining_minutes = (remaining_ms / 60_000.0).ceil() as u64;
            format!(
                " for about {remaining_minutes} minute{}",
                if remaining_minutes == 1 { "" } else { "s" }
            )
        }
    }
}
