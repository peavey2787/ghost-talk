use super::progress;
use ghost_api::KaspaArchivePublishResult;
use ghost_kaspa::wallet::{self, WalletSecret};
use ghost_kaspa::{
    estimate_archive_chunk_fee, ArchiveInFlightChunk, ArchiveProgress, ArchivePublishRequest,
    ARCHIVE_CHUNK_BYTES,
};
use ghost_media::{
    content_hash, decode_archive_chunk, encode_archive_chunk, KaspaArchiveChunk,
    KaspaArchiveLocator, MediaLocation, MediaReference,
};
use tauri::AppHandle;

pub(super) async fn publish_archive(
    app: &AppHandle,
    portal: &ghost_kaspa::PortalFacade,
    secret: &WalletSecret,
    request: &ArchivePublishRequest,
    address: &str,
    bytes: Vec<u8>,
) -> Result<KaspaArchivePublishResult, String> {
    let media_id = content_hash(&bytes);
    let chunks = bytes.chunks(ARCHIVE_CHUNK_BYTES).collect::<Vec<_>>();
    let count = u32::try_from(chunks.len()).map_err(|_| "archive has too many chunks")?;
    let mut state = load_or_create(app, request, address, &media_id, &bytes, &chunks, count)?;
    reconcile_in_flight(app, &mut state, &chunks).await?;
    ensure_cost_bound(portal, secret, request, address, &state, &chunks).await?;
    publish_remaining(app, portal, secret, &mut state, &chunks).await?;
    let result = build_publish_result(request, &state, bytes.len())?;
    progress::remove(app, &request.profile_id, &media_id)?;
    Ok(result)
}

fn load_or_create(
    app: &AppHandle,
    request: &ArchivePublishRequest,
    address: &str,
    media_id: &str,
    bytes: &[u8],
    chunks: &[&[u8]],
    count: u32,
) -> Result<ArchiveProgress, String> {
    let state = progress::load(app, &request.profile_id, media_id)?.unwrap_or_else(|| {
        ArchiveProgress::new(
            &request.profile_id,
            media_id,
            &request.content_type,
            address,
            bytes.len() as u64,
            count,
            request.public.clone(),
        )
    });
    progress::validate(
        &state,
        &request.profile_id,
        &request.content_type,
        address,
        bytes,
        chunks,
    )?;
    Ok(state)
}

async fn ensure_cost_bound(
    portal: &ghost_kaspa::PortalFacade,
    secret: &WalletSecret,
    request: &ArchivePublishRequest,
    address: &str,
    state: &ArchiveProgress,
    chunks: &[&[u8]],
) -> Result<(), String> {
    let paid = state.paid_fee()?;
    let remaining = &chunks[state.completed.len()..];
    let estimated =
        ghost_kaspa::archive_remaining_cost(portal, secret, &state.public, address, remaining)
            .await?;
    let projected = paid.saturating_add(u128::from(estimated));
    if request.max_cost_sompi == 0 || projected > u128::from(request.max_cost_sompi) {
        return Err(format!(
            "current archive estimate {projected} sompi including completed chunks exceeds the confirmed maximum {} sompi",
            request.max_cost_sompi
        ));
    }
    Ok(())
}

async fn publish_remaining(
    app: &AppHandle,
    portal: &ghost_kaspa::PortalFacade,
    secret: &WalletSecret,
    state: &mut ArchiveProgress,
    chunks: &[&[u8]],
) -> Result<(), String> {
    while state.completed.len() < chunks.len() {
        let index = state.completed.len();
        let body = chunks[index];
        let reserved_fee =
            estimate_archive_chunk_fee(portal, secret, &state.public, &state.address, body.len())
                .await?;
        state.mark_in_flight(index as u32, body, reserved_fee);
        progress::save(app, state)?;
        let payload = encode_archive_chunk(&state.media_id, index as u32, state.chunk_count, body)?;
        let result = match wallet::send_payload(
            portal,
            secret,
            &state.public,
            &state.address,
            reserved_fee,
            &payload,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                state.clear_in_flight();
                progress::save(app, state)?;
                return Err(error);
            }
        };
        commit_chunk(state, body, result)?;
        progress::save(app, state)?;
    }
    Ok(())
}

fn commit_chunk(
    state: &mut ArchiveProgress,
    body: &[u8],
    result: wallet::KaspaBroadcastResult,
) -> Result<(), String> {
    let index = state.completed.len() as u32;
    let txid = result.transaction_id.clone();
    let fee = result
        .fee_sompi
        .parse::<u128>()
        .map_err(|_| "archive transaction returned an invalid fee")?;
    let total = state.paid_fee()?.saturating_add(fee);
    state.completed.push(KaspaArchiveChunk {
        index,
        transaction_id: txid.clone(),
        content_hash: content_hash(body),
        size: body.len() as u32,
    });
    state.transaction_ids.push(txid);
    state.total_fee_sompi = total.to_string();
    state.public = result.public;
    state.clear_in_flight();
    Ok(())
}

async fn reconcile_in_flight(
    app: &AppHandle,
    state: &mut ArchiveProgress,
    chunks: &[&[u8]],
) -> Result<(), String> {
    let Some(in_flight) = state.in_flight.clone() else {
        return Ok(());
    };
    let rest =
        ghost_history::RestHistory::new(ghost_history::rest_base_for_network(&state.network), 64);
    let history = rest
        .wallet_history(std::slice::from_ref(&state.address))
        .await?;
    let recovered = recovered_in_flight_transaction(state, &in_flight, history.transactions);
    let Some((transaction_id, body)) = recovered else {
        return Err(
            "a previous archive chunk submission has an uncertain outcome; retry after Kaspa history reflects the transaction rather than risking a duplicate permanent publication"
                .into(),
        );
    };
    let expected = chunks
        .get(in_flight.index as usize)
        .ok_or("recovered archive chunk index is out of range")?;
    if body.as_slice() != *expected {
        return Err("recovered archive chunk does not match the requested media".into());
    }
    let recovered_total = state
        .paid_fee()?
        .saturating_add(u128::from(in_flight.reserved_fee_sompi));
    state.total_fee_sompi = recovered_total.to_string();
    state.fees_exact = false;
    state.completed.push(KaspaArchiveChunk {
        index: in_flight.index,
        transaction_id: transaction_id.clone(),
        content_hash: in_flight.content_hash,
        size: in_flight.size,
    });
    state.transaction_ids.push(transaction_id);
    state.clear_in_flight();
    progress::save(app, state)
}

fn recovered_in_flight_transaction(
    state: &ArchiveProgress,
    in_flight: &ArchiveInFlightChunk,
    transactions: Vec<ghost_history::HistoryTx>,
) -> Option<(String, Vec<u8>)> {
    transactions.into_iter().find_map(|transaction| {
        let Ok((media_id, index, count, body)) = decode_archive_chunk(&transaction.payload) else {
            return None;
        };
        let matches = media_id == state.media_id
            && index == in_flight.index
            && count == state.chunk_count
            && body.len() == in_flight.size as usize
            && content_hash(&body) == in_flight.content_hash;
        matches.then_some((transaction.transaction_id, body))
    })
}

fn build_publish_result(
    request: &ArchivePublishRequest,
    state: &ArchiveProgress,
    size: usize,
) -> Result<KaspaArchivePublishResult, String> {
    let locator = KaspaArchiveLocator {
        network: state.network.clone(),
        address: state.address.clone(),
        chunks: state.completed.clone(),
    };
    let reference = MediaReference {
        media_id: state.media_id.clone(),
        content_hash: state.media_id.clone(),
        content_type: request.content_type.clone(),
        size: size as u64,
        location: MediaLocation::KaspaArchive(locator.encode()?),
    };
    let projection = crate::wallet_commands::broadcast_projection(wallet::KaspaBroadcastResult {
        transaction_id: state.transaction_ids.last().cloned().unwrap_or_default(),
        fee_sompi: state.total_fee_sompi.clone(),
        public: state.public.clone(),
        timings: None,
    });
    Ok(KaspaArchivePublishResult {
        reference,
        transaction_ids: state.transaction_ids.clone(),
        actual_fee_sompi: state.total_fee_sompi.clone(),
        fee_is_exact: state.fees_exact,
        public: projection,
    })
}
