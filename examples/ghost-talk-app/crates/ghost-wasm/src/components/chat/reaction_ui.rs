use crate::model::ReactionKind;
use ghost_domain::reaction::MessageReaction;
use yew::prelude::*;

pub(crate) fn render_summary(
    reactions: &[MessageReaction],
    own_reaction: Option<ReactionKind>,
) -> Html {
    let mut chips = Vec::new();
    for kind in ReactionKind::ALL {
        let count = reactions.iter().filter(|entry| entry.kind == kind).count();
        if count == 0 {
            continue;
        }
        chips.push(html! {
            <span class={classes!("reaction-chip", (own_reaction == Some(kind)).then_some("mine"))}>
                {kind.emoji()}<small>{count}</small>
            </span>
        });
    }
    html! {<div class="reaction-summary">{for chips}</div>}
}

pub(crate) fn render_picker(
    own_reaction: Option<ReactionKind>,
    on_react: Callback<ReactionKind>,
) -> Html {
    html! {
        <details class="reaction-menu">
            <summary class="reaction-trigger" title="React" aria-label="React to message">{"👍"}</summary>
            <div class="reaction-picker" aria-label="Choose reaction">
                {for ReactionKind::ALL.into_iter().map(|kind| {
                    let callback = on_react.clone();
                    html! {
                        <button
                            type="button"
                            class={classes!("reaction-option", (own_reaction == Some(kind)).then_some("selected"))}
                            title={kind.label()}
                            onclick={Callback::from(move |_| callback.emit(kind))}
                        >{kind.emoji()}</button>
                    }
                })}
            </div>
        </details>
    }
}
