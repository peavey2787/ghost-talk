use yew::prelude::*;

#[component(GamesView)]
pub fn games_view() -> Html {
    let running = use_state(|| false);
    let toggle = {
        let running = running.clone();
        Callback::from(move |_| running.set(!*running))
    };
    html! {
      <section class="page">
        <div class="page-head"><div><h2>{"Games"}</h2><p>{"Deterministic Ghost Talk game sessions use the authenticated session channel."}</p></div></div>
        <div class="card game-card">
          <span class="empty-icon">{"◇"}</span>
          <div><b>{"Pong"}</b><small>{if *running {"Practice session running"} else {"Ready"}}</small></div>
          <button class={if *running {"on"} else {"primary"}} onclick={toggle}>{if *running {"Stop"} else {"Start practice"}}</button>
        </div>
      </section>
    }
}
