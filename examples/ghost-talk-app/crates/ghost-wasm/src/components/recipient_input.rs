use crate::model::Profile;
use web_sys::HtmlInputElement;
use yew::{prelude::*, AttrValue};

#[derive(Clone, Debug, PartialEq)]
struct Candidate {
    value: String,
    label: String,
    detail: String,
    score: u8,
}

#[derive(Properties, PartialEq)]
pub struct RecipientInputProps {
    pub profile: Profile,
    pub value: String,
    pub on_change: Callback<String>,
    #[prop_or_else(|| AttrValue::from("Contact, KNS, dot.k, or Kaspa address"))]
    pub placeholder: AttrValue,
}

fn contact_candidate(contact: &crate::model::Contact, query: &str) -> Option<Candidate> {
    let label = contact.label.trim();
    let kns = contact.kns_name.as_deref().unwrap_or("").trim();
    let dotk = contact.dotk_name.as_deref().unwrap_or("").trim();
    let address = contact.kaspa_address().trim();
    let score = match_score(query, &[label, kns, dotk, address])?;
    let alias = first_nonempty(kns, dotk, address);
    let value = if name_matches(query, dotk) {
        dotk.to_string()
    } else {
        first_nonempty(label, alias.as_str(), address)
    };
    Some(Candidate {
        value,
        label: if label.is_empty() {
            alias.clone()
        } else {
            label.to_string()
        },
        detail: alias,
        score,
    })
}

fn public_candidate(public: &crate::model::PublicGhostProfile, query: &str) -> Option<Candidate> {
    let name = if public.username.trim().is_empty() {
        public.display_name.trim()
    } else {
        public.username.trim()
    };
    let preferred = public.preferred_name().unwrap_or("").trim();
    let kns = public.kns_name.as_deref().unwrap_or("").trim();
    let dotk = public.dotk_name.as_deref().unwrap_or("").trim();
    let address = public.kaspa_address.trim();
    let score = match_score(query, &[name, preferred, kns, dotk, address])?;
    let alias = first_nonempty(preferred, kns, dotk);
    let alias = if alias.is_empty() {
        address.to_string()
    } else {
        alias
    };
    let value = if name_matches(query, dotk) {
        dotk.to_string()
    } else if name_matches(query, preferred) {
        preferred.to_string()
    } else {
        alias.clone()
    };
    Some(Candidate {
        value,
        label: if name.is_empty() {
            address.to_string()
        } else {
            name.to_string()
        },
        detail: alias,
        score: score.saturating_add(1),
    })
}

fn first_nonempty(first: &str, second: &str, third: &str) -> String {
    if !first.is_empty() {
        first.to_string()
    } else if !second.is_empty() {
        second.to_string()
    } else {
        third.to_string()
    }
}

fn recipient_candidates(profile: &Profile, raw_query: &str) -> Vec<Candidate> {
    let query = raw_query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut candidates = profile
        .contacts
        .iter()
        .filter_map(|contact| contact_candidate(contact, &query))
        .collect::<Vec<_>>();
    for candidate in profile
        .public_directory
        .iter()
        .filter_map(|public| public_candidate(public, &query))
    {
        if !candidates
            .iter()
            .any(|existing| existing.value.eq_ignore_ascii_case(&candidate.value))
        {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|a, b| {
        a.score.cmp(&b.score).then_with(|| {
            a.label
                .to_ascii_lowercase()
                .cmp(&b.label.to_ascii_lowercase())
        })
    });
    candidates.truncate(6);
    candidates
}

fn suggestion_list(candidates: Vec<Candidate>, on_change: Callback<String>) -> Html {
    if candidates.is_empty() {
        return Html::default();
    }
    html! {
        <div class="recipient-suggestions" role="listbox">
            {for candidates.into_iter().map(|candidate| {
                let value = candidate.value.clone();
                let on_change = on_change.clone();
                html! {
                    <button type="button" class="recipient-suggestion" onclick={Callback::from(move |_| on_change.emit(value.clone()))}>
                        <b>{candidate.label}</b>
                        <small>{candidate.detail}</small>
                    </button>
                }
            })}
        </div>
    }
}

#[component(RecipientInput)]
pub fn recipient_input(props: &RecipientInputProps) -> Html {
    let candidates = recipient_candidates(&props.profile, &props.value);
    let oninput = {
        let on_change = props.on_change.clone();
        Callback::from(move |event: InputEvent| {
            on_change.emit(event.target_unchecked_into::<HtmlInputElement>().value());
        })
    };
    html! {
        <div class="recipient-input-wrap">
            <input value={props.value.clone()} {oninput} placeholder={props.placeholder.clone()} autocomplete="off" />
            {suggestion_list(candidates, props.on_change.clone())}
        </div>
    }
}

fn name_matches(query: &str, value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let value = value.to_ascii_lowercase();
    value == query || value.starts_with(query) || value.contains(query)
}

fn match_score(query: &str, values: &[&str]) -> Option<u8> {
    let mut best: Option<u8> = None;
    for value in values {
        if value.is_empty() {
            continue;
        }
        let value = value.to_ascii_lowercase();
        let score = if value == query {
            0
        } else if value.starts_with(query) {
            1
        } else if value.contains(query) {
            3
        } else {
            continue;
        };
        best = Some(best.map_or(score, |current| current.min(score)));
    }
    best
}

fn exact_contact_target(profile: &Profile, value: &str) -> Result<Option<String>, String> {
    let matches = profile
        .contacts
        .iter()
        .filter(|contact| contact.label.trim().eq_ignore_ascii_case(value))
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(
            "Contact names must be unique. Rename the duplicate contacts before using that name."
                .into(),
        );
    }
    let Some(contact) = matches.first() else {
        return Ok(None);
    };
    if !contact.kaspa_address().trim().is_empty() {
        return Ok(Some(contact.kaspa_address().to_string()));
    }
    Ok(contact
        .kns_name
        .as_ref()
        .filter(|kns| !kns.trim().is_empty())
        .cloned()
        .or_else(|| {
            contact
                .dotk_name
                .as_ref()
                .filter(|name| !name.trim().is_empty())
                .cloned()
        }))
}

fn kns_contact_target(profile: &Profile, value: &str) -> Option<String> {
    profile.contacts.iter().find_map(|contact| {
        let matches = contact
            .kns_name
            .as_deref()
            .is_some_and(|kns| kns.eq_ignore_ascii_case(value));
        (matches && !contact.kaspa_address().trim().is_empty())
            .then(|| contact.kaspa_address().to_string())
    })
}

fn public_directory_target(profile: &Profile, value: &str) -> Option<String> {
    let matches = profile
        .public_directory
        .iter()
        .filter(|entry| public_name_matches(entry, value))
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].kaspa_address.clone())
}

fn public_name_matches(entry: &crate::model::PublicGhostProfile, value: &str) -> bool {
    entry
        .kns_name
        .as_deref()
        .is_some_and(|kns| kns.eq_ignore_ascii_case(value))
        || entry
            .dotk_name
            .as_deref()
            .is_some_and(|dotk| dotk.eq_ignore_ascii_case(value))
        || (!entry.username.trim().is_empty() && entry.username.eq_ignore_ascii_case(value))
        || (!entry.display_name.trim().is_empty() && entry.display_name.eq_ignore_ascii_case(value))
}

pub fn resolve_recipient_target(profile: &Profile, raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("Enter a contact name, KNS name, dot.k name, or Kaspa address.".into());
    }
    // A typed dot.k name must always reach the native resolver for a fresh deed proof.
    // Never replace it with a cached contact/public-directory address.
    if value.to_ascii_lowercase().ends_with(".k") {
        return Ok(value.to_string());
    }
    if let Some(target) = exact_contact_target(profile, value)? {
        return Ok(target);
    }
    if let Some(target) = kns_contact_target(profile, value) {
        return Ok(target);
    }
    if let Some(target) = public_directory_target(profile, value) {
        return Ok(target);
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests;
