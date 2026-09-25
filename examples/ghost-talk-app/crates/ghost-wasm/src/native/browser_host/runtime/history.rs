use ghost_api::{WalletHistoryEntry, WalletHistoryResult};
use ghost_kaspa::wallet::WalletPublic;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

pub(in crate::native::browser_host) async fn gather(
    public: &WalletPublic,
    rest_override: Option<&str>,
    priority: &[String],
) -> Result<WalletHistoryResult, String> {
    let addresses = ordered_addresses(public, priority);
    let base = rest_override
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| ghost_core::kaspa_rest_base(&public.network).to_owned());
    let mut unique = BTreeMap::<String, WalletHistoryEntry>::new();
    let mut warnings = Vec::new();
    let mut successes = 0usize;
    for address in addresses {
        match fetch_address(&base, &address).await {
            Ok(entries) => {
                successes += 1;
                for entry in entries {
                    merge_entry(&mut unique, entry);
                }
            }
            Err(error) => warnings.push(format!("{address}: {error}")),
        }
    }
    if successes == 0 && !warnings.is_empty() {
        return Err(format!(
            "Historical transaction requests failed for every wallet address; first failure: {}",
            warnings[0]
        ));
    }
    let mut history = unique.into_values().collect::<Vec<_>>();
    history.sort_by_key(|entry| std::cmp::Reverse(entry.blue_score.parse::<u64>().unwrap_or(0)));
    let mut used_addresses = history
        .iter()
        .flat_map(|entry| entry.addresses.iter().cloned())
        .collect::<Vec<_>>();
    used_addresses.sort();
    used_addresses.dedup();
    Ok(WalletHistoryResult {
        recommended_receive_index: recommended_receive_index(public, &used_addresses),
        history,
        warnings,
        used_addresses,
    })
}

fn ordered_addresses(public: &WalletPublic, priority: &[String]) -> Vec<String> {
    let wallet = public.all_addresses().cloned().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    priority
        .iter()
        .filter(|address| wallet.contains(*address))
        .chain(public.all_addresses())
        .filter(|address| seen.insert((*address).clone()))
        .cloned()
        .collect()
}

async fn fetch_address(base: &str, address: &str) -> Result<Vec<WalletHistoryEntry>, String> {
    Ok(fetch_raw_address(base, address).await?
        .iter()
        .filter_map(|entry| project_entry(address, entry))
        .collect())
}

pub(in crate::native::browser_host) async fn raw_payloads(
    network: &str,
    address: &str,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let values = fetch_raw_address(ghost_core::kaspa_rest_base(network), address).await?;
    Ok(values.into_iter().filter_map(|value| {
        let id = value.get("transaction_id")
            .or_else(|| value.get("transactionId"))?.as_str()?.to_owned();
        let payload = value.get("payload").and_then(Value::as_str)
            .and_then(|payload| hex::decode(payload).ok()).unwrap_or_default();
        Some((id, payload))
    }).collect())
}

pub(in crate::native::browser_host) async fn backup_payloads(
    network: &str,
    addresses: &[String],
    cutoff_ms: u64,
) -> Result<Vec<(u64, Vec<u8>)>, String> {
    let mut out = Vec::new();
    for address in addresses {
        for value in fetch_raw_address(ghost_core::kaspa_rest_base(network), address).await? {
            let block_time = value.get("block_time").or_else(|| value.get("blockTime")).and_then(json_u64);
            if !block_time.is_some_and(|time| time <= cutoff_ms) { continue; }
            let blue = value.get("accepting_block_blue_score")
                .or_else(|| value.get("acceptingBlockBlueScore")).and_then(json_u64).unwrap_or_default();
            let payload = value.get("payload").and_then(Value::as_str)
                .and_then(|payload| hex::decode(payload).ok()).unwrap_or_default();
            out.push((blue, payload));
        }
    }
    Ok(out)
}

async fn fetch_raw_address(base: &str, address: &str) -> Result<Vec<Value>, String> {
    const MAX_PAGES: usize = 64;
    let mut before = None::<u64>;
    let mut out = Vec::new();
    for _ in 0..MAX_PAGES {
        let url = history_url(base, address, before);
        let response = fetch(&url).await?;
        if !response.ok() {
            return Err(format!("history endpoint returned HTTP {}", response.status()));
        }
        let next = response.headers().get("x-next-page-before")
            .map_err(crate::native::invoke::js_error)?
            .and_then(|value| value.parse::<u64>().ok());
        let text = JsFuture::from(response.text().map_err(crate::native::invoke::js_error)?)
            .await.map_err(crate::native::invoke::js_error)?
            .as_string().ok_or("history response was not text")?;
        let entries: Vec<Value> = serde_json::from_str(&text)
            .map_err(|error| format!("invalid history response: {error}"))?;
        if entries.is_empty() { break; }
        out.extend(entries);
        let Some(next_before) = next else { break };
        if before == Some(next_before) {
            return Err("history pagination cursor did not advance".into());
        }
        before = Some(next_before);
    }
    Ok(out)
}

async fn fetch(url: &str) -> Result<Response, String> {
    let window = web_sys::window().ok_or("Browser window unavailable")?;
    JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(crate::native::invoke::js_error)?
        .dyn_into::<Response>()
        .map_err(|_| "history endpoint returned a non-HTTP response".to_string())
}

fn history_url(base: &str, address: &str, before: Option<u64>) -> String {
    let base = base.trim_end_matches('/');
    let mut url = format!(
        "{base}/addresses/{address}/full-transactions-page?limit=100&acceptance=accepted"
    );
    if let Some(before) = before {
        url.push_str("&before=");
        url.push_str(&before.to_string());
    }
    url
}

fn project_entry(address: &str, value: &Value) -> Option<WalletHistoryEntry> {
    let transaction_id = value
        .get("transaction_id")
        .or_else(|| value.get("transactionId"))?
        .as_str()?
        .to_owned();
    let blue_score = value
        .get("accepting_block_blue_score")
        .or_else(|| value.get("acceptingBlockBlueScore"))
        .and_then(json_u64);
    let block_time = value
        .get("block_time")
        .or_else(|| value.get("blockTime"))
        .and_then(json_u64);
    let payload = value
        .get("payload")
        .and_then(Value::as_str)
        .and_then(|payload| hex::decode(payload).ok())
        .unwrap_or_default();
    Some(ghost_kaspa::project_wallet_history_entry(
        transaction_id,
        blue_score,
        block_time,
        &payload,
        vec![address.to_owned()],
    ))
}

fn merge_entry(unique: &mut BTreeMap<String, WalletHistoryEntry>, mut entry: WalletHistoryEntry) {
    if let Some(existing) = unique.get_mut(&entry.transaction_id) {
        for address in entry.addresses.drain(..) {
            if !existing.addresses.contains(&address) {
                existing.addresses.push(address);
            }
        }
        return;
    }
    unique.insert(entry.transaction_id.clone(), entry);
}

fn recommended_receive_index(public: &WalletPublic, observed: &[String]) -> usize {
    if public.receive_addresses.is_empty() {
        return 0;
    }
    let observed = observed.iter().map(String::as_str).collect::<HashSet<_>>();
    let highest = public
        .receive_addresses
        .iter()
        .enumerate()
        .filter_map(|(index, address)| observed.contains(address.as_str()).then_some(index))
        .max();
    let last = public.receive_addresses.len() - 1;
    match highest {
        Some(index) if index >= public.next_receive_index => index.saturating_add(1).min(last),
        _ => public.next_receive_index.min(last),
    }
}

fn json_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}
