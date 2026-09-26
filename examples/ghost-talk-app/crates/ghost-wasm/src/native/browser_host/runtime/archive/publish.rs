use super::progress;
use ghost_api::{BroadcastResult, KaspaArchivePublishResult};
use ghost_kaspa::{
    wallet::{self, WalletSecret},
    PortalFacade,
};
use ghost_kaspa::{
    ArchiveInFlightChunk, ArchiveProgress, ArchivePublishRequest, ARCHIVE_CHUNK_BYTES,
};
use ghost_media::{
    content_hash, decode_archive_chunk, encode_archive_chunk, KaspaArchiveChunk,
    KaspaArchiveLocator, MediaLocation, MediaReference,
};

pub(in crate::native::browser_host) async fn archive(
    portal: &PortalFacade,
    secret: &WalletSecret,
    request: &ArchivePublishRequest,
    address: &str,
    bytes: Vec<u8>,
) -> Result<KaspaArchivePublishResult, String> {
    let media_id = content_hash(&bytes);
    let chunks = bytes.chunks(ARCHIVE_CHUNK_BYTES).collect::<Vec<_>>();
    let count = u32::try_from(chunks.len()).map_err(|_| "archive has too many chunks")?;
    let mut state = progress::load(&request.profile_id, &media_id)?.unwrap_or_else(|| {
        ArchiveProgress::new(
            &request.profile_id,
            &media_id,
            &request.content_type,
            address,
            bytes.len() as u64,
            count,
            request.public.clone(),
        )
    });
    validate_progress(&state, request, address, &bytes, &chunks)?;
    reconcile_in_flight(&mut state, &chunks).await?;
    ensure_cost(portal, secret, request, &state, &chunks).await?;
    publish_remaining(portal, secret, &mut state, &chunks).await?;
    let result = result(request, &state, bytes.len())?;
    progress::remove(&request.profile_id, &media_id)?;
    Ok(result)
}

fn validate_progress(
    state: &ArchiveProgress,
    request: &ArchivePublishRequest,
    address: &str,
    bytes: &[u8],
    chunks: &[&[u8]],
) -> Result<(), String> {
    let matches = state.profile_id == request.profile_id
        && state.content_type == request.content_type
        && state.address == address
        && state.total_size == bytes.len() as u64
        && state.media_id == content_hash(bytes)
        && state.chunk_count as usize == chunks.len()
        && state.completed.len() == state.transaction_ids.len();
    if !matches {
        return Err("saved archive progress does not match the requested media".into());
    }
    for (position, completed) in state.completed.iter().enumerate() {
        let body = chunks
            .get(position)
            .ok_or("saved archive progress exceeds media chunks")?;
        if completed.index != position as u32
            || completed.transaction_id != state.transaction_ids[position]
            || completed.size as usize != body.len()
            || completed.content_hash != content_hash(body)
        {
            return Err("saved archive progress chunk metadata is inconsistent".into());
        }
    }
    Ok(())
}

async fn reconcile_in_flight(state: &mut ArchiveProgress, chunks: &[&[u8]]) -> Result<(), String> {
    let Some(in_flight) = state.in_flight.clone() else {
        return Ok(());
    };
    let payloads = super::super::history::raw_payloads(&state.network, &state.address).await?;
    let found = payloads.into_iter().find_map(|(txid, payload)| {
        let Ok((media_id, index, count, body)) = decode_archive_chunk(&payload) else {
            return None;
        };
        let matches = media_id == state.media_id
            && index == in_flight.index
            && count == state.chunk_count
            && body.len() == in_flight.size as usize
            && content_hash(&body) == in_flight.content_hash;
        matches.then_some((txid, body))
    });
    let Some((transaction_id, body)) = found else {
        return Err("a previous archive chunk submission has an uncertain outcome; retry after Kaspa history reflects the transaction rather than risking a duplicate permanent publication".into());
    };
    let expected = chunks
        .get(in_flight.index as usize)
        .ok_or("recovered archive chunk index is out of range")?;
    if body.as_slice() != *expected {
        return Err("recovered archive chunk does not match the requested media".into());
    }
    let total = state
        .paid_fee()?
        .saturating_add(u128::from(in_flight.reserved_fee_sompi));
    state.total_fee_sompi = total.to_string();
    state.fees_exact = false;
    state.completed.push(KaspaArchiveChunk {
        index: in_flight.index,
        transaction_id: transaction_id.clone(),
        content_hash: in_flight.content_hash,
        size: in_flight.size,
    });
    state.transaction_ids.push(transaction_id);
    state.in_flight = None;
    progress::save(state)
}

async fn ensure_cost(
    portal: &PortalFacade,
    secret: &WalletSecret,
    request: &ArchivePublishRequest,
    state: &ArchiveProgress,
    chunks: &[&[u8]],
) -> Result<(), String> {
    let remaining = ghost_kaspa::archive_remaining_cost(
        portal,
        secret,
        &state.public,
        &state.address,
        &chunks[state.completed.len()..],
    )
    .await?;
    let projected = state.paid_fee()?.saturating_add(u128::from(remaining));
    if request.max_cost_sompi == 0 || projected > u128::from(request.max_cost_sompi) {
        return Err(format!("current archive estimate {projected} sompi including completed chunks exceeds the confirmed maximum {} sompi", request.max_cost_sompi));
    }
    Ok(())
}

async fn publish_remaining(
    portal: &PortalFacade,
    secret: &WalletSecret,
    state: &mut ArchiveProgress,
    chunks: &[&[u8]],
) -> Result<(), String> {
    while state.completed.len() < chunks.len() {
        let index = state.completed.len();
        let body = chunks[index];
        let fee = ghost_kaspa::estimate_archive_chunk_fee(
            portal,
            secret,
            &state.public,
            &state.address,
            body.len(),
        )
        .await?;
        state.in_flight = Some(ArchiveInFlightChunk {
            index: index as u32,
            content_hash: content_hash(body),
            size: body.len() as u32,
            reserved_fee_sompi: fee,
        });
        progress::save(state)?;
        let payload = encode_archive_chunk(&state.media_id, index as u32, state.chunk_count, body)?;
        let sent =
            wallet::send_payload(portal, secret, &state.public, &state.address, fee, &payload)
                .await?;
        let actual = sent
            .fee_sompi
            .parse::<u128>()
            .map_err(|_| "archive transaction returned an invalid fee")?;
        state.total_fee_sompi = state.paid_fee()?.saturating_add(actual).to_string();
        state.completed.push(KaspaArchiveChunk {
            index: index as u32,
            transaction_id: sent.transaction_id.clone(),
            content_hash: content_hash(body),
            size: body.len() as u32,
        });
        state.transaction_ids.push(sent.transaction_id);
        state.public = sent.public;
        state.in_flight = None;
        progress::save(state)?;
    }
    Ok(())
}

fn result(
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
    Ok(KaspaArchivePublishResult {
        reference,
        transaction_ids: state.transaction_ids.clone(),
        actual_fee_sompi: state.total_fee_sompi.clone(),
        fee_is_exact: state.fees_exact,
        public: BroadcastResult {
            transaction_id: state.transaction_ids.last().cloned().unwrap_or_default(),
            fee_sompi: state.total_fee_sompi.clone(),
            public: state.public.projection(),
        },
    })
}
