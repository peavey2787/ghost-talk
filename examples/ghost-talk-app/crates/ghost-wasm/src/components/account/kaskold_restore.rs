use crate::components::form::{binary_file_input, text_input, textarea_input};
use web_sys::HtmlSelectElement;
use yew::prelude::*;

#[derive(Clone, Properties, PartialEq)]
pub(super) struct KasKoldRestoreProps {
    pub(super) mnemonic: UseStateHandle<String>,
    pub(super) passphrase: UseStateHandle<String>,
}

#[component(KasKoldRestoreImport)]
pub(super) fn kaskold_restore_import(props: &KasKoldRestoreProps) -> Html {
    let kind = use_state(|| "mnemonic".to_string());
    let text = use_state(String::new);
    let bytes = use_state(Vec::<u8>::new);
    let credential = use_state(String::new);
    let status = use_state(String::new);
    let busy = use_state(|| false);
    let file_import = matches!(kind.as_str(), "portable" | "stego" | "recovery");

    html! {
        <section class="kaskold-restore-panel">
            <div class="kaskold-restore-head">
                <div><b>{"Import from KasKold"}</b><small>{"Use KasKold recovery material to fill the Ghost Talk recovery root."}</small></div>
                <span class="compat-badge">{"KasKold"}</span>
            </div>
            <select value={(*kind).clone()} onchange={select_kind(kind.clone(), status.clone())}>
                <option value="mnemonic">{"Recovery words"}</option>
                <option value="seedqr">{"SeedQR text"}</option>
                <option value="portable">{"Portable .kwp backup"}</option>
                <option value="stego">{"Steganographic JPEG backup"}</option>
                <option value="recovery">{"Recovery material / Compact SeedQR file"}</option>
            </select>
            {if file_import {
                html! { <input type="file" accept={file_accept(&kind)} onchange={binary_file_input(bytes.clone(), status.clone())}/> }
            } else {
                html! { <textarea rows="3" value={(*text).clone()} oninput={textarea_input(text.clone())} placeholder="Paste KasKold recovery material"/> }
            }}
            {if matches!(kind.as_str(), "portable" | "stego") {
                html! { <input type="password" value={(*credential).clone()} oninput={text_input(credential.clone())} placeholder="KasKold backup password"/> }
            } else {
                Html::default()
            }}
            <button type="button" disabled={*busy} onclick={import_callback(
                (*props).clone(), kind.clone(), text.clone(), bytes.clone(), credential.clone(), status.clone(), busy.clone()
            )}>{if *busy {"Importing…"} else {"Import from KasKold"}}</button>
            <small>{"Ghost Talk IDs use a 24-word root. If the KasKold wallet used a BIP39 passphrase, enter the same passphrase below before restoring."}</small>
            {if status.is_empty() {Html::default()} else {html! {<p class="status">{(*status).clone()}</p>}}}
        </section>
    }
}

fn file_accept(kind: &str) -> &'static str {
    if kind == "stego" {
        "image/jpeg"
    } else {
        ".kwp,.bin,.txt,application/octet-stream,text/plain"
    }
}

fn select_kind(kind: UseStateHandle<String>, status: UseStateHandle<String>) -> Callback<Event> {
    Callback::from(move |event: Event| {
        kind.set(event.target_unchecked_into::<HtmlSelectElement>().value());
        status.set(String::new());
    })
}

fn import_callback(
    props: KasKoldRestoreProps,
    kind: UseStateHandle<String>,
    text: UseStateHandle<String>,
    bytes: UseStateHandle<Vec<u8>>,
    credential: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        if *busy {
            return;
        }
        busy.set(true);
        let result = import_recovery(
            kind.as_str(),
            text.as_str(),
            bytes.as_slice(),
            credential.as_str(),
            props.passphrase.as_str(),
        );
        match result {
            Ok(words) => {
                props.mnemonic.set(words);
                status.set(
                    "KasKold recovery imported. Review the 24 words and restore the Ghost Talk ID."
                        .into(),
                );
            }
            Err(error) => status.set(error),
        }
        busy.set(false);
    })
}

type RestoreHandler =
    fn(&mut vault_runtime::VaultRuntime, &str, &[u8], &str, &str) -> Result<(), String>;

const RESTORE_HANDLERS: [(&str, RestoreHandler); 5] = [
    ("mnemonic", restore_mnemonic),
    ("seedqr", restore_seedqr),
    ("portable", restore_portable),
    ("stego", restore_stego),
    ("recovery", restore_material),
];

fn import_recovery(
    kind: &str,
    text: &str,
    bytes: &[u8],
    credential: &str,
    passphrase: &str,
) -> Result<String, String> {
    let mut runtime = vault_runtime::VaultRuntime::new();
    let handler = RESTORE_HANDLERS
        .iter()
        .find_map(|(name, handler)| (*name == kind).then_some(*handler))
        .ok_or_else(|| "Unsupported KasKold restore format".to_string())?;
    handler(&mut runtime, text, bytes, credential, passphrase)?;
    recovery_words(&runtime)
}

fn restore_mnemonic(
    runtime: &mut vault_runtime::VaultRuntime,
    text: &str,
    _bytes: &[u8],
    _credential: &str,
    passphrase: &str,
) -> Result<(), String> {
    runtime
        .add_restored_wallet(text.trim(), passphrase)
        .map(|_| ())
        .map_err(restore_error)
}

fn restore_seedqr(
    runtime: &mut vault_runtime::VaultRuntime,
    text: &str,
    _bytes: &[u8],
    _credential: &str,
    passphrase: &str,
) -> Result<(), String> {
    runtime
        .add_recovery_material(text.trim().as_bytes(), passphrase)
        .map(|_| ())
        .map_err(restore_error)
}

fn restore_portable(
    runtime: &mut vault_runtime::VaultRuntime,
    _text: &str,
    bytes: &[u8],
    credential: &str,
    _passphrase: &str,
) -> Result<(), String> {
    runtime
        .add_portable_backup(bytes, credential)
        .map(|_| ())
        .map_err(restore_error)
}

fn restore_stego(
    runtime: &mut vault_runtime::VaultRuntime,
    _text: &str,
    bytes: &[u8],
    credential: &str,
    _passphrase: &str,
) -> Result<(), String> {
    runtime
        .add_stego_backup(bytes, credential)
        .map(|_| ())
        .map_err(restore_error)
}

fn restore_material(
    runtime: &mut vault_runtime::VaultRuntime,
    _text: &str,
    bytes: &[u8],
    _credential: &str,
    passphrase: &str,
) -> Result<(), String> {
    runtime
        .add_recovery_material(bytes, passphrase)
        .map(|_| ())
        .map_err(restore_error)
}

fn restore_error(error: vault_runtime::VaultRuntimeError) -> String {
    format!("KasKold restore: {error:?}")
}

fn recovery_words(runtime: &vault_runtime::VaultRuntime) -> Result<String, String> {
    let words = runtime
        .backup_recovery_phrase()
        .map_err(|error| format!("KasKold recovery phrase: {error:?}"))?
        .to_string();
    if words.split_whitespace().count() != 24 {
        return Err("Ghost Talk restore requires a 24-word KasKold recovery root.".into());
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::import_recovery;

    const WORDS: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

    #[test]
    fn kaskold_restore_accepts_24_word_recovery_and_rejects_12_word_root() {
        let words = import_recovery("mnemonic", WORDS, &[], "", "").expect("24-word import");
        assert_eq!(words.split_whitespace().count(), 24);
        let short = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        assert!(import_recovery("mnemonic", short, &[], "", "").is_err());
    }
}
