use yew::prelude::*;

use crate::{
    components::account::identity::ActivatedProfile,
    components::form::{binary_file_input, text_input},
};

mod actions;
mod io;
use actions::{backup_callback, BackupInputs};
use io::{render_status, select_state};

#[derive(Properties, PartialEq)]
pub(super) struct KasKoldBackupProps {
    pub(super) session: ActivatedProfile,
}

#[component(KasKoldBackup)]
pub(super) fn kaskold_backup(props: &KasKoldBackupProps) -> Html {
    let kind = use_state(|| "words".to_string());
    let secret = use_state(String::new);
    let carrier = use_state(Vec::<u8>::new);
    let result = use_state(String::new);
    let status = use_state(String::new);
    let busy = use_state(|| false);

    html! {
        <div class="card kaskold-backup-card">
            <div class="kaskold-backup-head">
                <div><h3>{"KasKold-compatible backup"}</h3><p>{"Create a KasKold recovery format from this Ghost Talk wallet."}</p></div>
                <span class="compat-badge">{"KasKold"}</span>
            </div>
            <div class="kaskold-backup-controls">
                <select value={(*kind).clone()} onchange={select_state(kind.clone())}>
                    <option value="words">{"Recovery words"}</option>
                    <option value="seedqr">{"SeedQR"}</option>
                    <option value="compact-seedqr">{"Compact SeedQR"}</option>
                    <option value="xprv">{"Account XPrv"}</option>
                    <option value="portable">{"Portable encrypted .kwp"}</option>
                    <option value="portable-xprv">{"Portable XPrv .kwp"}</option>
                    <option value="stego">{"Steganographic JPEG"}</option>
                </select>
                {if kind.as_str() == "stego" {
                    html! { <input type="file" accept="image/jpeg" onchange={binary_file_input(carrier.clone(), status.clone())}/> }
                } else {
                    Html::default()
                }}
                {if matches!(kind.as_str(), "portable" | "portable-xprv" | "stego") {
                    html! { <input type="password" value={(*secret).clone()} oninput={text_input(secret.clone())} placeholder="Backup password"/> }
                } else {
                    Html::default()
                }}
                <button disabled={*busy} onclick={backup_callback(
                    props.session.profile.clone(), props.session.password.clone(),
                    BackupInputs { kind: kind.clone(), secret: secret.clone(), carrier: carrier.clone(), result: result.clone(), status: status.clone(), busy: busy.clone() }
                )}>{if *busy {"Creating…"} else {"Create backup"}}</button>
            </div>
            {render_status(&status, &result)}
        </div>
    }
}
