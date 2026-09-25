use crate::model::{Profile, ProfilePatch};
use ghost_api::KaspaArchivePlan;
use ghost_media::MediaReference;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(super) struct ArchivePanelProps {
    pub(super) profile: Profile,
    pub(super) password: String,
    pub(super) recording: UseStateHandle<Option<MediaReference>>,
    pub(super) status: UseStateHandle<String>,
    pub(super) on_update: Callback<ProfilePatch>,
}

#[component(ArchivePanel)]
pub(super) fn archive_panel(props: &ArchivePanelProps) -> Html {
    let plan = use_state(|| None::<KaspaArchivePlan>);
    let confirmed = use_state(|| false);
    let planning = use_state(|| false);
    let publishing = use_state(|| false);
    let plan_action = plan_callback(props, plan.clone(), confirmed.clone(), planning.clone());
    let publish_action =
        publish_callback(props, plan.clone(), confirmed.clone(), publishing.clone());
    html! {
        <div class="card form-grid">
            <h3>{"Permanent Kaspa archive"}</h3>
            <small>{"⛓ Kaspa publication is permanent. Cost is estimated from the current wallet-aware transaction planner before confirmation."}</small>
            {recording_summary(&props.recording)}
            <button type="button" disabled={props.recording.is_none() || *planning || *publishing} onclick={plan_action}>
                {if *planning { "Calculating…" } else { "Calculate current archive cost" }}
            </button>
            {plan_view(&plan)}
            {confirmation(&plan, confirmed.clone())}
            <button type="button" class="primary" disabled={plan.is_none() || !*confirmed || *publishing} onclick={publish_action}>
                {if *publishing { "Publishing permanently…" } else { "Confirm permanent archive" }}
            </button>
        </div>
    }
}

fn plan_callback(
    props: &ArchivePanelProps,
    plan: UseStateHandle<Option<KaspaArchivePlan>>,
    confirmed: UseStateHandle<bool>,
    planning: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let recording = props.recording.clone();
    let status = props.status.clone();
    Callback::from(move |_| {
        let Some(reference) = recording.as_ref().cloned() else {
            return;
        };
        planning.set(true);
        confirmed.set(false);
        let profile = profile.clone();
        let password = password.clone();
        let plan = plan.clone();
        let planning = planning.clone();
        let status = status.clone();
        spawn_local(async move {
            match crate::controllers::media::archive_plan(&profile, &password, &reference).await {
                Ok(value) => {
                    plan.set(Some(value));
                    status.set("Archive cost calculated from the current Kaspa planner.".into());
                }
                Err(error) => status.set(error),
            }
            planning.set(false);
        });
    })
}

fn publish_callback(
    props: &ArchivePanelProps,
    plan: UseStateHandle<Option<KaspaArchivePlan>>,
    confirmed: UseStateHandle<bool>,
    publishing: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let recording = props.recording.clone();
    let status = props.status.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        let Some(reference) = recording.as_ref().cloned() else {
            return;
        };
        let Some(cost) = plan.as_ref().map(|value| value.estimated_cost_sompi) else {
            return;
        };
        if !*confirmed {
            return;
        }
        publishing.set(true);
        let profile = profile.clone();
        let password = password.clone();
        let recording = recording.clone();
        let status = status.clone();
        let on_update = on_update.clone();
        let publishing = publishing.clone();
        spawn_local(publish_archive(ArchivePublishTask {
            profile,
            password,
            reference,
            cost,
            recording,
            status,
            on_update,
            publishing,
        }));
    })
}

struct ArchivePublishTask {
    profile: Profile,
    password: String,
    reference: MediaReference,
    cost: u64,
    recording: UseStateHandle<Option<MediaReference>>,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
    publishing: UseStateHandle<bool>,
}

async fn publish_archive(task: ArchivePublishTask) {
    match crate::controllers::media::archive_publish(
        &task.profile,
        &task.password,
        &task.reference,
        task.cost,
    )
    .await
    {
        Ok(outcome) => {
            task.on_update.emit(outcome.patch);
            task.recording.set(Some(outcome.reference));
            let fee_label = if outcome.fee_is_exact {
                "actual fee"
            } else {
                "accounted fee after recovered submission"
            };
            task.status.set(format!(
                "Permanent archive complete · {} transactions · {} sompi {}.",
                outcome.transaction_count, outcome.actual_fee_sompi, fee_label
            ));
        }
        Err(error) => task.status.set(error),
    }
    task.publishing.set(false);
}

fn plan_view(plan: &UseStateHandle<Option<KaspaArchivePlan>>) -> Html {
    let Some(plan) = plan.as_ref() else {
        return Html::default();
    };
    html! {
        <div class="archive-plan">
            <b>{"Current estimate"}</b>
            <small>{format!("Encoded size: {} bytes", plan.total_bytes)}</small>
            <small>{format!("Chunks / transactions: {}", plan.transactions)}</small>
            <small>{format!("Estimated fee per transaction: up to {} sompi", plan.estimated_fee_per_tx_sompi)}</small>
            <strong>{format!("Estimated total: {} sompi", plan.estimated_cost_sompi)}</strong>
        </div>
    }
}

fn confirmation(
    plan: &UseStateHandle<Option<KaspaArchivePlan>>,
    confirmed: UseStateHandle<bool>,
) -> Html {
    if plan.is_none() {
        return Html::default();
    }
    let change = {
        let confirmed = confirmed.clone();
        Callback::from(move |event: Event| {
            let input = event.target_unchecked_into::<web_sys::HtmlInputElement>();
            confirmed.set(input.checked());
        })
    };
    html! {
        <label class="checkbox-row warning"><input type="checkbox" checked={*confirmed} onchange={change}/>{"I understand this publication is permanent and authorize spending up to the estimate above."}</label>
    }
}

fn recording_summary(recording: &UseStateHandle<Option<MediaReference>>) -> Html {
    recording
        .as_ref()
        .map(|media| {
            html! {
                <small>{format!("Media {} · {} bytes", media.media_id, media.size)}</small>
            }
        })
        .unwrap_or_default()
}
