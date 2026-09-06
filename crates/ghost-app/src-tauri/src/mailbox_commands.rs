use ghost_history::{rest_base_for_network, DagObservation, RestHistory};
use ghost_kaspa::{
    wallet::{WalletPublic, MAILBOX_OUTPUT_SOMPI},
    LiveTransactionObservation, PortalFacade,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};

const MAILBOX_SCAN_INTERVAL: Duration = Duration::from_millis(100);
const MAILBOX_PRIORITY_HISTORY_INTERVAL: Duration = Duration::from_secs(60);
const MAILBOX_BROAD_HISTORY_INTERVAL: Duration = Duration::from_secs(10 * 60);
const WALLET_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(3);
const MAILBOX_HISTORY_OVERLAP: u64 = 2_048;

#[derive(Clone, Debug, Serialize)]
pub struct MailboxEvent {
    pub transaction_id: String,
    pub blue_score: String,
    pub payload_hex: String,
    pub block_time: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailboxSyncResult {
    pub checkpoint: String,
    pub directory_checkpoint: String,
    pub observations: Vec<MailboxEvent>,
    pub public_profiles: Vec<super::peer_commands::PublicGhostProfile>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailboxSendResult {
    pub transaction_id: String,
    pub fee_sompi: String,
    pub mailbox_output_sompi: String,
    pub public: WalletPublic,
    pub pending_handshake: bool,
    pub pending_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct WalletLiveEvent {
    profile_id: String,
    snapshot: Option<super::wallet_commands::WalletSnapshot>,
    checkpoint: String,
    mailbox: Vec<MailboxEvent>,
}

#[derive(Clone, Debug, Serialize)]
struct DirectoryLiveEvent {
    profile_id: String,
    directory_checkpoint: String,
    public_profiles: Vec<super::peer_commands::PublicGhostProfile>,
}

#[derive(Clone, Debug, Serialize)]
struct NetworkStatusEvent {
    profile_id: String,
    status: &'static str,
    reconnect_attempts: u32,
}

#[derive(Default)]
pub struct MonitorState {
    generations: Arc<Mutex<HashMap<String, u64>>>,
    /// Latest public HD cursor state for each running profile. The monitor task
    /// must not be restarted just because receive/change indices rotate, but it
    /// also must not keep scanning a stale advertised receive index forever.
    publics: Arc<Mutex<HashMap<String, WalletPublic>>>,
}

struct LiveBlockStream {
    blocks: tokio::sync::broadcast::Receiver<ghost_kaspa::LiveBlockEvent>,
}

#[tauri::command]
pub async fn mailbox_send_message(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    _app: AppHandle,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    contact_id: String,
    destination: String,
    body: String,
    message_id: String,
    stego_profile: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
    reuse_change: bool,
) -> Result<MailboxSendResult, String> {
    let _ = &sender_display_name; // retained for stable Tauri command compatibility
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("message body is empty or exceeds the Ghost Talk event limit".into());
    }
    super::hydra_commands::parse_stego_profile(&stego_profile)?;
    if message_id.len() != 32 || !message_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk message id must be exactly 32 hexadecimal characters".into());
    }
    let outbound_lock = wallet_state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    crate::debug_log::record(
        "info",
        "message",
        "send-requested",
        format!("profile={} peer={} message={} destination={} reuse_change={}", profile_id, contact_id, message_id, destination, reuse_change),
    );
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox fee")?;
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;

    // HYDRA sessions are deliberately memory-only. If the peer session is
    // already active, seal the message immediately. Otherwise create/reuse a
    // first-contact handshake offer and remember the plaintext only in the
    // unlocked in-memory runtime until the authenticated answer arrives.
    let runtime = hydra_state.runtime(&profile_id)?;
    let (payloads, pending_id) = {
        let mut runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        if runtime.blocked_peers.contains(&contact_id) {
            return Err("This peer ended the current chat session. Start a new chat to establish a fresh KKTP SID before sending again.".into());
        }
        // finish_handshake() installs the initiator ratchet before the prepared
        // pq_finish carrier has necessarily been observed by the responder.
        // Until that exact FINISH is peer-confirmed, preparing any ordinary
        // active-session message could consume seq=1 while the responder is still
        // waiting for seq=0 inside pq_finish. Fail closed and replay the cached
        // FINISH instead of advancing the ratchet/KKTP sequence.
        if runtime
            .prepared_completion
            .as_ref()
            .is_some_and(|prepared| prepared.contact_id == contact_id)
        {
            return Err(
                "The secure-session FINISH is still awaiting the peer's signed acknowledgement; wait for handshake completion before sending another message"
                    .into(),
            );
        }
        if runtime
            .prepared_recovery_finish
            .as_ref()
            .is_some_and(|prepared| prepared.contact_id == contact_id)
        {
            return Err(
                "The secure-session recovery FINISH is still awaiting Kaspa broadcast; wait for handshake recovery before sending another message"
                    .into(),
            );
        }
        let hydra_status = runtime.hydra.session_status(&contact_id)?;
        match hydra_status.as_str() {
            "active" => {
                let use_kktp = runtime
                    .kktp_sessions
                    .get(&contact_id)
                    .is_some_and(|binding| binding.state == super::hydra_commands::KktpSessionState::Active);
                let prepared = if use_kktp && reuse_change {
                    // Realtime route signaling/live media stays outside the strict durable
                    // KKTP text sequence, but it is still bound to the exact
                    // active conversation SID both outside and inside HYDRA.
                    // This prevents same-peer historical/parallel sessions from
                    // being routed into the currently selected voice call.
                    super::hydra_commands::prepare_realtime_mailbox(
                        &mut runtime,
                        &contact_id,
                        &message_id,
                        &body,
                    )?
                } else if use_kktp {
                    if let Some(cached) = runtime.prepared_kktp_deliveries.get(&message_id).cloned() {
                        if cached.contact_id != contact_id
                            || cached.body != body
                            || cached.stego_profile != stego_profile
                        {
                            return Err(
                                "The logical KKTP message id is already bound to different prepared content"
                                    .into(),
                            );
                        }
                        // Retry the exact same SID/seq/HYDRA ciphertext. Do not
                        // touch the ratchet or KKTP counter again after an actual
                        // or ambiguous failed submit.
                        super::hydra_commands::PreparedMailbox {
                            payloads_hex: cached.payloads_hex,
                        }
                    } else {
                        if runtime.prepared_kktp_deliveries.len() >= super::hydra_commands::MAX_PREPARED_KKTP_DELIVERIES {
                            return Err(
                                "Too many unresolved KKTP message submissions are retained in memory; retry failed sends or restart the session"
                                    .into(),
                            );
                        }
                        let prepared = super::hydra_commands::prepare_kktp_mailbox(
                            &mut runtime,
                            &contact_id,
                            &message_id,
                            &body,
                            &stego_profile,
                        )?;
                        runtime.prepared_kktp_deliveries.insert(
                            message_id.clone(),
                            super::hydra_commands::PreparedKktpDelivery {
                                contact_id: contact_id.clone(),
                                body: body.clone(),
                                stego_profile: stego_profile.clone(),
                                payloads_hex: prepared.payloads_hex.clone(),
                            },
                        );
                        prepared
                    }
                } else {
                    // Legacy pre-KKTP in-memory sessions retain the old generic
                    // HYDRA mailbox wrapper. Current KKTP realtime traffic above
                    // always uses the SID-bound GTR1 carrier.
                    let sender_identity_id = runtime.identity_id.clone();
                    super::hydra_commands::prepare_mailbox_inner(
                        &mut runtime.hydra,
                        &sender_identity_id,
                        &contact_id,
                        &message_id,
                        &body,
                        &stego_profile,
                    )?
                };
                runtime.pending_outbound = None;
                runtime.prepared_completion = None;
                let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
                (payloads, None)
            }
            "missing" | "closed" => {
                if !runtime.hydra.has_contact(&contact_id)? {
                    return Err("The recipient has not accepted this Ghost Talk chat yet".into());
                }
                let existing = runtime.kktp_sessions.get(&contact_id).cloned();
                if existing.as_ref().is_some_and(|binding| {
                    binding.role == super::hydra_commands::KktpRole::Responder
                        && matches!(
                            binding.state,
                            super::hydra_commands::KktpSessionState::Discovered
                                | super::hydra_commands::KktpSessionState::Handshake
                        )
                }) {
                    return Err("Waiting for the chat initiator's authenticated PQ handshake".into());
                }
                let sid = existing
                    .filter(|binding| {
                        binding.role == super::hydra_commands::KktpRole::Initiator
                            && binding.state == super::hydra_commands::KktpSessionState::Discovered
                    })
                    .map(|binding| binding.sid)
                    .unwrap_or_else(super::hydra_commands::fresh_kktp_sid);
                runtime.hydra.abort_handshake(&contact_id)?;
                super::hydra_commands::install_kktp_binding(
                    &mut runtime,
                    &contact_id,
                    sid.clone(),
                    super::hydra_commands::KktpRole::Initiator,
                    super::hydra_commands::KktpSessionState::Handshake,
                )?;
                let offer = runtime.hydra.init_handshake(&contact_id)?;
                let binding = runtime
                    .kktp_sessions
                    .get(&contact_id)
                    .cloned()
                    .ok_or_else(|| "KKTP initiator binding disappeared".to_string())?;
                let control = super::hydra_commands::kktp_handshake_payload(
                    &runtime,
                    &binding,
                    "pq_init",
                    &offer,
                    None,
                )?;
                let payloads_hex = super::hydra_commands::frame_control(control)?.payloads_hex;
                let pending_id = ghost_core::Id128::new_random().to_string();
                runtime.pending_outbound = Some(super::hydra_commands::PendingOutbound {
                    id: pending_id.clone(),
                    sid: sid.clone(),
                    contact_id: contact_id.clone(),
                    destination: destination.clone(),
                    body: body.clone(),
                    message_id: message_id.clone(),
                    stego_profile: stego_profile.clone(),
                    offer_payloads_hex: payloads_hex.clone(),
                });
                runtime.prepared_completion = None;
                crate::debug_log::record(
                    "info",
                    "handshake",
                    "pq-init-prepared",
                    format!("profile={} sid={} peer={} pending={} message={}", profile_id, sid, contact_id, pending_id, message_id),
                );
                let payloads = decode_mailbox_payloads(&payloads_hex)?;
                (payloads, Some(pending_id))
            }
            "pending" => {
                let pending = runtime.pending_outbound.as_ref().ok_or_else(|| {
                    "A secure-session handshake is already pending for this peer; retry after it completes"
                        .to_string()
                })?;
                if pending.contact_id != contact_id
                    || pending.destination != destination
                    || pending.body != body
                    || pending.message_id != message_id
                    || pending.stego_profile != stego_profile
                {
                    return Err(
                        "The secure session is still being established for a previous message"
                            .into(),
                    );
                }
                let payloads = decode_mailbox_payloads(&pending.offer_payloads_hex)?;
                (payloads, Some(pending.id.clone()))
            }
            _ => return Err("HYDRA returned an unknown session state".into()),
        }
    };

    // KKTP sequence and HYDRA ratchet state advance when an active-session
    // ciphertext is prepared. Retain the exact carrier only while SubmitTransaction
    // is unresolved so an actual/ambiguous failure can retry byte-for-byte without
    // burning another sequence number. A successful Kaspa submit commits an ordinary
    // active-session message; peer-signed acknowledgements are reserved for pq_finish
    // bootstrap confirmation rather than doubling every chat message transaction.
    // First-contact offers are likewise cached and re-broadcast byte-for-byte on
    // explicit retry.
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        reuse_change,
    )
    .await?;

    crate::debug_log::record(
        "info",
        if pending_id.is_some() { "handshake" } else { "message" },
        if pending_id.is_some() { "carrier-send-accepted-by-kaspa" } else { "message-send-accepted-by-kaspa" },
        format!("profile={} peer={} message={} pending={} txid={}", profile_id, contact_id, message_id, pending_id.as_deref().unwrap_or("none"), result.transaction_id),
    );

    // Base KKTP uses one on-chain mailbox carrier per active-session message.
    // Once the node accepts that exact transaction, release its retry cache.
    // pq_finish is different: its prepared_completion state is intentionally
    // retained until the responder's authenticated ACK proves session activation.
    if pending_id.is_none() && !reuse_change {
        let runtime = hydra_state.runtime(&profile_id)?;
        runtime
            .lock()
            .await
            .prepared_kktp_deliveries
            .remove(&message_id);
    }

    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: pending_id.is_some(),
        pending_id,
    })
}

#[tauri::command]
pub async fn mailbox_send_contact_request(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    request_id: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    if request_id.len() != 32 || !request_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk contact-request id must be exactly 32 hexadecimal characters".into());
    }
    ghost_kaspa::validate_destination(&destination)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "discovery-send-start",
        format!("profile={} sid={} destination={} identity={}", profile_id, request_id, destination, identity_id),
    );
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let runtime = hydra_state.runtime(&profile_id)?;
    let descriptor = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        let card = runtime.hydra.contact_card()?;
        super::peer_commands::build_private_descriptor(
            &secret,
            &public,
            &card,
            &identity_id,
            &sender_display_name,
        )?
    };
    let mut request = ghost_protocol::GhostContactRequest {
        version: ghost_protocol::GHOST_KKTP_VERSION,
        request_id: request_id.clone(),
        recipient_kaspa_address: destination.clone(),
        sender: descriptor,
        signature_hex: String::new(),
    };
    let mut signing_key = ghost_kaspa::wallet::receive_private_key(&secret, 0)?;
    let signed = ghost_kaspa::sign_contact_request(&mut request, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    let prepared = super::hydra_commands::frame_control(request.encode()?)?;
    let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox contact-request fee")?;
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;
    crate::debug_log::record(
        "info",
        "handshake",
        "discovery-send-accepted-by-kaspa",
        format!("profile={} sid={} txid={}", profile_id, request_id, result.transaction_id),
    );
    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: true,
        pending_id: Some(request_id),
    })
}

#[tauri::command]
pub async fn mailbox_send_contact_accept(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    signed_request_hex: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    crate::debug_log::record(
        "info",
        "handshake",
        "response-send-start",
        format!("profile={} identity={} signed_request_bytes={}", profile_id, identity_id, signed_request_hex.len() / 2),
    );
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let local_kaspa_addresses: Vec<String> = public
        .receive_addresses
        .iter()
        .chain(public.change_addresses.iter())
        .cloned()
        .collect();
    let runtime = hydra_state.runtime(&profile_id)?;
    let (request, descriptor) = {
        let mut runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        // Re-verify the exact signed GTCR at acceptance time. This makes ignored
        // requests restart-safe without trusting mutable frontend peer fields or
        // pre-populating HYDRA's persistent contact roster with unsolicited spam.
        let request = super::hydra_commands::accept_signed_contact_request(
            &mut runtime,
            &signed_request_hex,
            &local_kaspa_addresses,
        )?;
        let card = runtime.hydra.contact_card()?;
        let descriptor = super::peer_commands::build_private_descriptor(
            &secret,
            &public,
            &card,
            &identity_id,
            &sender_display_name,
        )?;
        (request, descriptor)
    };
    let destination = request.sender.kaspa_address.clone();
    let acceptor_kaspa_address = request.recipient_kaspa_address.clone();
    crate::debug_log::record(
        "info",
        "handshake",
        "response-request-verified",
        format!("profile={} sid={} peer={} destination={} acceptor={}", profile_id, request.request_id, request.sender.hydra_identity_id, destination, acceptor_kaspa_address),
    );
    ghost_kaspa::validate_destination(&destination)?;
    ghost_kaspa::validate_destination(&acceptor_kaspa_address)?;
    let mut accepted = ghost_protocol::GhostContactAccept {
        version: ghost_protocol::GHOST_KKTP_VERSION,
        request_id: request.request_id,
        recipient_kaspa_address: destination.clone(),
        acceptor_kaspa_address: acceptor_kaspa_address.clone(),
        responder: descriptor,
        signature_hex: String::new(),
    };
    let mut signing_key = ghost_kaspa::wallet::private_key_for_address(
        &secret,
        &public,
        &acceptor_kaspa_address,
    )?;
    let signed = ghost_kaspa::sign_contact_accept(&mut accepted, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    let prepared = super::hydra_commands::frame_control(accepted.encode()?)?;
    let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox contact-accept fee")?;
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;
    crate::debug_log::record(
        "info",
        "handshake",
        "response-send-accepted-by-kaspa",
        format!("profile={} sid={} txid={} destination={}", profile_id, accepted.request_id, result.transaction_id, destination),
    );
    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: true,
        pending_id: None,
    })
}

#[tauri::command]
pub async fn mailbox_send_session_end(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    contact_id: String,
    destination: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    ghost_kaspa::validate_destination(&destination)?;
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let sender_kaspa_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let runtime = hydra_state.runtime(&profile_id)?;
    let mut end = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        super::hydra_commands::prepare_kktp_session_end(
            &runtime,
            &contact_id,
            &sender_kaspa_address,
            &destination,
            "left",
        )?
    };
    let mut signing_key = ghost_kaspa::wallet::receive_private_key(&secret, 0)?;
    let signed = ghost_kaspa::sign_kktp_session_end(&mut end, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    let prepared = super::hydra_commands::frame_control(end.encode()?)?;
    let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox session_end fee")?;
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;
    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: false,
        pending_id: None,
    })
}

#[tauri::command]
pub async fn mailbox_send_control(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    payloads_hex: Vec<String>,
    completes_pending_id: Option<String>,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox control fee")?;
    let payloads = decode_mailbox_payloads(&payloads_hex)?;
    let carrier = reassemble_mailbox_payloads(&payloads)?;
    let legacy_hydra_control = carrier.len() >= 5 && &carrier[..4] == b"GTH1";
    let kktp_pq_control = carrier.starts_with(ghost_protocol::KKTP_ANCHOR_PREFIX)
        && ghost_protocol::kktp_anchor_type(&carrier)?.as_deref() == Some("ghost_handshake")
        && ghost_protocol::KktpHandshakeControl::decode(&carrier).is_ok();
    if !legacy_hydra_control && !kktp_pq_control {
        return Err("mailbox control carrier is not an authenticated KKTP/HYDRA control packet".into());
    }
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;

    if let Ok(runtime) = hydra_state.runtime(&profile_id) {
        let mut runtime = runtime.lock().await;
        if completes_pending_id.is_some() {
            // Keep the exact prepared FINISH packet until the responder's signed
            // acknowledgement proves the full fragmented control was observed.
        } else {
            let matches = runtime
                .prepared_recovery_finish
                .as_ref()
                .is_some_and(|value| value.payloads_hex == payloads_hex);
            if matches {
                runtime.pending_recovery = None;
                runtime.prepared_recovery_finish = None;
            }
        }
    }

    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: false,
        pending_id: None,
    })
}

#[tauri::command]
pub async fn mailbox_retry_handshake_finish(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    contact_id: String,
    message_id: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    if message_id.len() != 32 || !message_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk message id must be exactly 32 hexadecimal characters".into());
    }
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox FINISH retry fee")?;
    let runtime = hydra_state.runtime(&profile_id)?;
    let (destination, payloads) = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        let prepared = runtime
            .prepared_completion
            .as_ref()
            .ok_or_else(|| "no peer-unacknowledged KKTP FINISH is retained for retry".to_string())?;
        if prepared.contact_id != contact_id || prepared.message_id != message_id {
            return Err("retained KKTP FINISH does not match this chat message".into());
        }
        let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
        (prepared.destination.clone(), payloads)
    };
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;
    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: true,
        pending_id: None,
    })
}

#[tauri::command]
pub async fn mailbox_send_recovery_offer(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    offer_hex: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    ghost_kaspa::validate_destination(&destination)?;
    let offer = hex::decode(&offer_hex)
        .map_err(|_| "HYDRA recovery offer is not valid hex".to_string())?;
    if offer.is_empty() || offer.len() > 64 * 1024 {
        return Err("HYDRA recovery offer size is invalid".into());
    }
    let _ = &sender_display_name; // retained for stable Tauri command compatibility
    let runtime = hydra_state.runtime(&profile_id)?;
    let payloads = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        let pending = runtime.pending_recovery.as_ref().ok_or_else(|| {
            "KKTP secure-session recovery is no longer pending".to_string()
        })?;
        if pending.destination != destination || pending.offer_hex != offer_hex {
            return Err("KKTP recovery offer does not match the pending peer recovery".into());
        }
        let binding = runtime
            .kktp_sessions
            .get(&pending.contact_id)
            .cloned()
            .ok_or_else(|| "KKTP recovery session binding is missing".to_string())?;
        if binding.sid != pending.sid
            || binding.role != super::hydra_commands::KktpRole::Initiator
        {
            return Err("KKTP recovery session identity changed before broadcast".into());
        }
        let control = super::hydra_commands::kktp_handshake_payload(
            &runtime,
            &binding,
            "pq_init",
            &offer,
            None,
        )?;
        let prepared = super::hydra_commands::frame_control(control)?;
        decode_mailbox_payloads(&prepared.payloads_hex)?
    };
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox recovery fee")?;
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;
    {
        let runtime = hydra_state.runtime(&profile_id)?;
        let mut runtime = runtime.lock().await;
        if let Some(pending) = runtime.pending_recovery.as_mut() {
            if pending.destination == destination && pending.offer_hex == offer_hex {
                pending.offer_broadcast = true;
            }
        }
    }
    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: true,
        pending_id: None,
    })
}

#[tauri::command]
pub async fn mailbox_send_delivery_ack(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, super::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, super::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    destination_hydra_id: String,
    message_id: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    if message_id.len() != 32 || !message_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk message id must be exactly 32 hexadecimal characters".into());
    }
    if destination_hydra_id.len() != 64
        || !destination_hydra_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("HYDRA destination id must be exactly 64 hexadecimal characters".into());
    }
    ghost_kaspa::validate_destination(&destination)?;
    let secret = wallet_state.secret_or_open(&profile_id, &password, &sealed, &public)?;
    {
        let runtime = hydra_state.runtime(&profile_id)?;
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        if !runtime.hydra.has_contact(&destination_hydra_id)? {
            return Err("delivery acknowledgement destination is not a known HYDRA peer".into());
        }
        let route = runtime
            .peer_routes
            .get(&destination_hydra_id)
            .ok_or_else(|| "delivery acknowledgement destination has no verified Kaspa peer route".to_string())?;
        if route.kaspa_address != destination {
            return Err("delivery acknowledgement destination does not match the verified peer route".into());
        }
    }
    let signer_kaspa_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let mut ack = ghost_protocol::GhostDeliveryAck {
        version: 1,
        signer_kaspa_address,
        signer_hydra_id: identity_id,
        destination_hydra_id,
        message_id,
        signature_hex: String::new(),
    };
    let mut private_key = ghost_kaspa::wallet::receive_private_key(&secret, 0)?;
    let signed = ghost_kaspa::sign_delivery_ack(&mut ack, &private_key);
    zeroize::Zeroize::zeroize(&mut private_key);
    signed?;
    let prepared = super::hydra_commands::frame_control(ack.encode()?)?;
    let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "mailbox delivery acknowledgement fee")?;
    let portal = outbound_portal(
        &gateway,
        &public,
        wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = send_mailbox_payloads_via_gateway(
        &gateway,
        &portal,
        &secret,
        &public,
        &destination,
        fee,
        &payloads,
        false,
    )
    .await?;
    Ok(MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: result.public,
        pending_handshake: false,
        pending_id: None,
    })
}

#[tauri::command]
pub async fn mailbox_sync(
    public: WalletPublic,
    checkpoint: String,
    directory_checkpoint: Option<String>,
    rest_endpoint: Option<String>,
) -> Result<MailboxSyncResult, String> {
    sync_mailbox(
        &public,
        &checkpoint,
        directory_checkpoint.as_deref().unwrap_or("0"),
        rest_endpoint.as_deref(),
    ).await
}

#[tauri::command]
pub fn wallet_monitor_start(
    app: AppHandle,
    state: State<'_, MonitorState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    directory: State<'_, super::peer_commands::PublicDirectoryState>,
    profile_id: String,
    public: WalletPublic,
    checkpoint: String,
    directory_checkpoint: Option<String>,
    rest_endpoint: Option<String>,
    wrpc_endpoint: Option<String>,
) -> Result<(), String> {
    // Directory live state now advances from the authoritative BlockAdded stream;
    // retain the command argument for persisted-state compatibility without
    // pretending it participates in the live connection lifecycle.
    let _ = directory_checkpoint;

    let generation = {
        let mut generations = state
            .generations
            .lock()
            .map_err(|_| "wallet monitor state is poisoned".to_string())?;
        let generation = generations
            .get(&profile_id)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        generations.insert(profile_id.clone(), generation);
        generation
    };
    state
        .publics
        .lock()
        .map_err(|_| "wallet monitor public state is poisoned".to_string())?
        .insert(profile_id.clone(), public.clone());

    // Private mailbox delivery is latency-sensitive and address-targeted. Keep
    // it independent from global Discover indexing so a slow directory backfill
    // can never delay an incoming request/message that is already in address history.
    let mailbox_generations = state.generations.clone();
    let mailbox_app = app.clone();
    let mailbox_profile_id = profile_id.clone();
    let mailbox_public = public.clone();
    let mailbox_publics = state.publics.clone();
    let mailbox_rest_endpoint = rest_endpoint.clone();
    let mailbox_wrpc_endpoint = wrpc_endpoint.clone();
    let mailbox_gateway = gateway.inner().clone();
    let mailbox_directory = directory.inner().clone();
    tauri::async_runtime::spawn(async move {
        let mut checkpoint = checkpoint;
        let mut snapshot = None;
        let mut recovery_needed = true;
        let mut priority_recovery_complete = false;
        let mut broad_recovery_complete = false;
        let mut priority_history_sync: Option<
            tokio::task::JoinHandle<Result<PrivateMailboxSync, String>>,
        > = None;
        let mut broad_history_sync: Option<
            tokio::task::JoinHandle<Result<PrivateMailboxSync, String>>,
        > = None;
        let mut snapshot_sync: Option<
            tokio::task::JoinHandle<Option<super::wallet_commands::WalletSnapshot>>,
        > = None;
        let monitor_started = Instant::now();
        let mut next_priority_history_scan = monitor_started;
        let mut next_snapshot_scan = monitor_started + Duration::from_millis(500);
        let mut next_broad_history_scan = monitor_started + Duration::from_secs(2);

        // The authoritative live route is the selected Kaspa node's BlockAdded
        // stream itself. Its full block payload is decoded on the same wRPC
        // connection used for sends/RPC calls. REST address history is recovery
        // only and never gates or filters a live carrier.
        let mut live_stream = None;
        let mut live_reconnect: Option<tokio::task::JoinHandle<Result<LiveBlockStream, String>>> =
            None;
        let mut reconnect_attempts = 0u32;
        let mut next_live_retry = Instant::now();
        let mut last_network_status = "connecting";
        emit_network_status(&mailbox_app, &mailbox_profile_id, "connecting", 0);
        loop {
            if !monitor_generation_is_active(&mailbox_generations, &mailbox_profile_id, generation) {
                break;
            }

            // Priority mailbox history, broad catch-up, and wallet snapshot refresh
            // are three independent jobs. In particular, a slow balance/history REST
            // request must never hold a signed contact Response or PQ control carrier
            // hostage. The stable first-contact/current addresses are polled on the
            // latency path while a much slower broad address sweep remains the durable
            // recovery path for older/rotated addresses.
            if priority_history_sync.as_ref().is_some_and(|task| task.is_finished()) {
                if let Some(task) = priority_history_sync.take() {
                    match task.await {
                        Ok(Ok(mailbox)) => {
                            priority_recovery_complete = true;
                            advance_mailbox_checkpoint(&mut checkpoint, &mailbox.checkpoint);
                            if !mailbox.observations.is_empty() {
                                let txids = mailbox.observations.iter()
                                    .map(|event| event.transaction_id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                crate::debug_log::record(
                                    "info",
                                    "mailbox",
                                    "priority-history-carriers-observed",
                                    format!("profile={} count={} txids={} checkpoint={}", mailbox_profile_id, mailbox.observations.len(), txids, checkpoint),
                                );
                                let _ = mailbox_app.emit(
                                    "ghost://wallet-live",
                                    WalletLiveEvent {
                                        profile_id: mailbox_profile_id.clone(),
                                        snapshot: snapshot.clone(),
                                        checkpoint: checkpoint.clone(),
                                        mailbox: mailbox.observations,
                                    },
                                );
                            }
                        }
                        Ok(Err(error)) => {
                            priority_recovery_complete = false;
                            crate::debug_log::record(
                                "warn",
                                "mailbox",
                                "priority-history-sync-failed",
                                format!("profile={} error={}", mailbox_profile_id, error),
                            );
                        }
                        Err(_) => priority_recovery_complete = false,
                    }
                }
            }

            if broad_history_sync.as_ref().is_some_and(|task| task.is_finished()) {
                if let Some(task) = broad_history_sync.take() {
                    match task.await {
                        Ok(Ok(mailbox)) => {
                            broad_recovery_complete = true;
                            advance_mailbox_checkpoint(&mut checkpoint, &mailbox.checkpoint);
                            if !mailbox.observations.is_empty() {
                                let txids = mailbox.observations.iter()
                                    .map(|event| event.transaction_id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                crate::debug_log::record(
                                    "info",
                                    "mailbox",
                                    "broad-history-carriers-observed",
                                    format!("profile={} count={} txids={} checkpoint={}", mailbox_profile_id, mailbox.observations.len(), txids, checkpoint),
                                );
                                let _ = mailbox_app.emit(
                                    "ghost://wallet-live",
                                    WalletLiveEvent {
                                        profile_id: mailbox_profile_id.clone(),
                                        snapshot: snapshot.clone(),
                                        checkpoint: checkpoint.clone(),
                                        mailbox: mailbox.observations,
                                    },
                                );
                            }
                        }
                        Ok(Err(error)) => {
                            broad_recovery_complete = false;
                            crate::debug_log::record(
                                "warn",
                                "mailbox",
                                "broad-history-sync-failed",
                                format!("profile={} error={}", mailbox_profile_id, error),
                            );
                        }
                        Err(_) => broad_recovery_complete = false,
                    }
                }
            }

            if snapshot_sync.as_ref().is_some_and(|task| task.is_finished()) {
                if let Some(task) = snapshot_sync.take() {
                    if let Ok(Some(new_snapshot)) = task.await {
                        snapshot = Some(new_snapshot);
                        let _ = mailbox_app.emit(
                            "ghost://wallet-live",
                            WalletLiveEvent {
                                profile_id: mailbox_profile_id.clone(),
                                snapshot: snapshot.clone(),
                                checkpoint: checkpoint.clone(),
                                mailbox: Vec::new(),
                            },
                        );
                    }
                }
            }

            // Never await endpoint discovery/connection inside the authoritative
            // mailbox loop. A dead public node must not freeze the UI/recovery
            // loop while the gateway tries replacement candidates sequentially.
            if live_stream.is_none() {
                let reconnect_finished = live_reconnect
                    .as_ref()
                    .is_some_and(|task| task.is_finished());
                if reconnect_finished {
                    if let Some(task) = live_reconnect.take() {
                        match task.await {
                            Ok(Ok(live)) => {
                                live_stream = Some(live);
                                reconnect_attempts = 0;
                                next_live_retry = Instant::now();
                            }
                            Ok(Err(error)) => {
                                let delay = live_retry_delay(reconnect_attempts);
                                crate::debug_log::record(
                                    "warn",
                                    "kaspa",
                                    "live-block-stream-connect-failed",
                                    format!(
                                        "profile={} attempt={} retry_ms={} error={}",
                                        mailbox_profile_id,
                                        reconnect_attempts,
                                        delay.as_millis(),
                                        error
                                    ),
                                );
                                next_live_retry = Instant::now() + delay;
                                let portal_connected = mailbox_gateway
                                    .is_connected_to(&mailbox_public.network)
                                    .await;
                                if portal_connected {
                                    if last_network_status != "connected" {
                                        emit_network_status(
                                            &mailbox_app,
                                            &mailbox_profile_id,
                                            "connected",
                                            0,
                                        );
                                        last_network_status = "connected";
                                    }
                                } else if last_network_status != "reconnecting" {
                                    emit_network_status(
                                        &mailbox_app,
                                        &mailbox_profile_id,
                                        "reconnecting",
                                        reconnect_attempts,
                                    );
                                    last_network_status = "reconnecting";
                                }
                            }
                            Err(error) => {
                                let delay = live_retry_delay(reconnect_attempts);
                                crate::debug_log::record(
                                    "error",
                                    "kaspa",
                                    "live-block-stream-task-failed",
                                    format!(
                                        "profile={} attempt={} retry_ms={} error={}",
                                        mailbox_profile_id,
                                        reconnect_attempts,
                                        delay.as_millis(),
                                        error
                                    ),
                                );
                                next_live_retry = Instant::now() + delay;
                                let portal_connected = mailbox_gateway
                                    .is_connected_to(&mailbox_public.network)
                                    .await;
                                if portal_connected {
                                    if last_network_status != "connected" {
                                        emit_network_status(
                                            &mailbox_app,
                                            &mailbox_profile_id,
                                            "connected",
                                            0,
                                        );
                                        last_network_status = "connected";
                                    }
                                } else if last_network_status != "reconnecting" {
                                    emit_network_status(
                                        &mailbox_app,
                                        &mailbox_profile_id,
                                        "reconnecting",
                                        reconnect_attempts,
                                    );
                                    last_network_status = "reconnecting";
                                }
                            }
                        }
                    }
                }
                if live_stream.is_none()
                    && live_reconnect.is_none()
                    && Instant::now() >= next_live_retry
                {
                    reconnect_attempts = reconnect_attempts.saturating_add(1);
                    let status = if reconnect_attempts == 1 { "connecting" } else { "reconnecting" };
                    // The live BlockAdded stream is authoritative for new carriers.
                    // REST mailbox history is recovery-only and must never mask a dead
                    // live Kaspa gateway as a healthy connection.
                    if last_network_status != "connected" {
                        emit_network_status(
                            &mailbox_app,
                            &mailbox_profile_id,
                            status,
                            reconnect_attempts.saturating_sub(1),
                        );
                        last_network_status = status;
                    }
                    let reconnect_public = mailbox_public.clone();
                    let reconnect_endpoint = mailbox_wrpc_endpoint.clone();
                    let reconnect_profile = mailbox_profile_id.clone();
                    let reconnect_gateway = mailbox_gateway.clone();
                    live_reconnect = Some(tokio::spawn(async move {
                        start_live_block_stream(
                            &reconnect_profile,
                            &reconnect_public,
                            reconnect_endpoint.as_deref(),
                            &reconnect_gateway,
                        )
                        .await
                    }));
                }
            }

            let current_public = mailbox_publics
                .lock()
                .ok()
                .and_then(|publics| publics.get(&mailbox_profile_id).cloned())
                .unwrap_or_else(|| mailbox_public.clone());

            // Kaspa connectivity and the Ghost Talk BlockAdded subscription are
            // deliberately separate. A Portal that passed its live DAA health RPC
            // is connected even while the mailbox subscription is retrying.
            let kaspa_network_ok = mailbox_gateway
                .is_connected_to(&mailbox_public.network)
                .await;
            let live_network_ok = live_stream.is_some();

            if priority_history_sync.is_none()
                && (recovery_needed || live_stream.is_none())
                && Instant::now() >= next_priority_history_scan
            {
                let history_public = current_public.clone();
                let history_checkpoint = checkpoint.clone();
                let history_rest_endpoint = mailbox_rest_endpoint.clone();
                next_priority_history_scan = Instant::now() + MAILBOX_PRIORITY_HISTORY_INTERVAL;
                priority_history_sync = Some(tokio::spawn(async move {
                    sync_priority_private_mailbox(
                        &history_public,
                        &history_checkpoint,
                        history_rest_endpoint.as_deref(),
                    )
                    .await
                }));
            }

            if broad_history_sync.is_none()
                && (recovery_needed || live_stream.is_none())
                && Instant::now() >= next_broad_history_scan
            {
                let history_public = current_public.clone();
                let history_checkpoint = checkpoint.clone();
                let history_rest_endpoint = mailbox_rest_endpoint.clone();
                next_broad_history_scan = Instant::now() + MAILBOX_BROAD_HISTORY_INTERVAL;
                broad_history_sync = Some(tokio::spawn(async move {
                    sync_private_mailbox(
                        &history_public,
                        &history_checkpoint,
                        history_rest_endpoint.as_deref(),
                    )
                    .await
                }));
            }

            if snapshot_sync.is_none() && Instant::now() >= next_snapshot_scan {
                let snapshot_public = current_public.clone();
                let snapshot_rest_endpoint = mailbox_rest_endpoint.clone();
                let snapshot_wrpc_endpoint = mailbox_wrpc_endpoint.clone();
                let snapshot_gateway = mailbox_gateway.clone();
                next_snapshot_scan = Instant::now() + WALLET_SNAPSHOT_INTERVAL;
                snapshot_sync = Some(tokio::spawn(async move {
                    super::wallet_commands::refresh_wallet(
                        &snapshot_gateway,
                        &snapshot_public,
                        snapshot_wrpc_endpoint.as_deref(),
                        snapshot_rest_endpoint.as_deref(),
                    )
                    .await
                    .ok()
                }));
            }
            if live_network_ok && priority_recovery_complete && broad_recovery_complete {
                recovery_needed = false;
            }
            let network_ok = kaspa_network_ok;

            if network_ok {
                // BlockAdded on the selected Kaspa wRPC connection is the live
                // path. Address history is only durable startup/disconnect recovery.
                if last_network_status != "connected" {
                    emit_network_status(&mailbox_app, &mailbox_profile_id, "connected", 0);
                    last_network_status = "connected";
                }
            }
            // While the monitor is alive and has a scheduled/in-flight retry, it
            // is reconnecting rather than terminally disconnected. Do not clobber
            // the yellow/orange progress state with a red disconnected event in
            // the same loop iteration that launched the connection task.

            if let Some(live) = live_stream.as_mut() {
                match tokio::time::timeout(
                    MAILBOX_SCAN_INTERVAL,
                    live.blocks.recv(),
                )
                .await
                {
                    Ok(Ok(event)) => {
                        let observations = event.observations;
                        if !observations.is_empty() {
                            let txids = observations
                                .iter()
                                .map(|observation| observation.txid.as_str())
                                .collect::<Vec<_>>()
                                .join(",");
                            crate::debug_log::record(
                                "info",
                                "mailbox",
                                "live-block-carriers-observed",
                                format!(
                                    "profile={} block={} daa={} count={} txids={}",
                                    mailbox_profile_id,
                                    event.block_hash,
                                    event.daa_score,
                                    observations.len(),
                                    txids,
                                ),
                            );
                            for observation in &observations {
                                mailbox_directory.ingest_payload(
                                    &observation.payload,
                                    observation.daa_score,
                                    event.daa_score,
                                );
                            }

                            // Public profile announcements are current-state Ghost
                            // carriers too. Learn them directly from the same live
                            // Kaspa block rather than querying the REST API for the
                            // current DAG.
                            let public_profiles =
                                live_public_profile_updates(&observations, event.daa_score);
                            if !public_profiles.is_empty() {
                                let _ = mailbox_app.emit(
                                    "ghost://directory-live",
                                    DirectoryLiveEvent {
                                        profile_id: mailbox_profile_id.clone(),
                                        directory_checkpoint: event.daa_score.to_string(),
                                        public_profiles,
                                    },
                                );
                            }

                            let _ = mailbox_app.emit(
                                "ghost://wallet-live",
                                WalletLiveEvent {
                                    profile_id: mailbox_profile_id.clone(),
                                    snapshot: snapshot.clone(),
                                    checkpoint: checkpoint.clone(),
                                    mailbox: observations
                                        .into_iter()
                                        .map(to_live_mailbox_event)
                                        .collect(),
                                },
                            );
                        }
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                        // A lagged local receiver is never silently ignored. Trigger
                        // durable recovery immediately, but keep the one healthy Kaspa
                        // gateway connection and subscription alive.
                        crate::debug_log::record(
                            "error",
                            "mailbox",
                            "live-block-consumer-lagged",
                            format!("profile={} skipped_blocks={}", mailbox_profile_id, skipped),
                        );
                        recovery_needed = true;
                        priority_recovery_complete = false;
                        broad_recovery_complete = false;
                        next_priority_history_scan = Instant::now();
                        next_broad_history_scan = Instant::now();
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                        crate::debug_log::record(
                            "warn",
                            "mailbox",
                            "live-block-subscription-closed",
                            format!("profile={}", mailbox_profile_id),
                        );
                        live_stream = None;
                        let portal_connected = mailbox_gateway
                            .is_connected_to(&mailbox_public.network)
                            .await;
                        if portal_connected {
                            if last_network_status != "connected" {
                                emit_network_status(
                                    &mailbox_app,
                                    &mailbox_profile_id,
                                    "connected",
                                    0,
                                );
                                last_network_status = "connected";
                            }
                        } else if last_network_status != "reconnecting" {
                            emit_network_status(
                                &mailbox_app,
                                &mailbox_profile_id,
                                "reconnecting",
                                reconnect_attempts.saturating_add(1),
                            );
                            last_network_status = "reconnecting";
                        }
                        next_live_retry = Instant::now();
                        recovery_needed = true;
                        priority_recovery_complete = false;
                        broad_recovery_complete = false;
                        next_priority_history_scan = Instant::now();
                        next_broad_history_scan = Instant::now();
                    }
                    Err(_) => {
                        // No block arrived during this 100 ms scheduling slice.
                        // This is normal and does not trigger any REST fallback.
                    }
                }
            } else {
                tokio::time::sleep(MAILBOX_SCAN_INTERVAL).await;
            }
        }
        if let Some(task) = live_reconnect {
            task.abort();
        }
        if let Some(task) = priority_history_sync {
            task.abort();
        }
        if let Some(task) = broad_history_sync {
            task.abort();
        }
        if let Some(task) = snapshot_sync {
            task.abort();
        }
    });

    // Current Discover/profile updates are emitted directly from the same live
    // BlockAdded stream above. No periodic REST scan of the current DAG exists.
    Ok(())
}

fn monitor_generation_is_active(
    generations: &Arc<Mutex<HashMap<String, u64>>>,
    profile_id: &str,
    generation: u64,
) -> bool {
    generations
        .lock()
        .map(|active| active.get(profile_id).copied() == Some(generation))
        .unwrap_or(false)
}

#[tauri::command]
pub fn wallet_monitor_update_public(
    state: State<'_, MonitorState>,
    profile_id: String,
    public: WalletPublic,
) -> Result<(), String> {
    state
        .publics
        .lock()
        .map_err(|_| "wallet monitor public state is poisoned".to_string())?
        .insert(profile_id, public);
    Ok(())
}

#[tauri::command]
pub fn wallet_monitor_stop(
    state: State<'_, MonitorState>,
    profile_id: String,
) -> Result<(), String> {
    state
        .generations
        .lock()
        .map_err(|_| "wallet monitor state is poisoned".to_string())?
        .remove(&profile_id);
    state
        .publics
        .lock()
        .map_err(|_| "wallet monitor public state is poisoned".to_string())?
        .remove(&profile_id);
    Ok(())
}

fn live_retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_secs((1u64 << exponent).min(30))
}

fn emit_network_status(app: &AppHandle, profile_id: &str, status: &'static str, reconnect_attempts: u32) {
    let _ = app.emit(
        "ghost://network-status",
        NetworkStatusEvent {
            profile_id: profile_id.to_owned(),
            status,
            reconnect_attempts,
        },
    );
}

pub(super) async fn outbound_portal(
    gateway: &super::kaspa_gateway::KaspaGatewayState,
    public: &WalletPublic,
    wrpc_override: Option<&str>,
) -> Result<PortalFacade, String> {
    gateway.portal(public, wrpc_override).await
}

fn decode_mailbox_payloads(payloads_hex: &[String]) -> Result<Vec<Vec<u8>>, String> {
    if payloads_hex.is_empty() || payloads_hex.len() > ghost_core::MAX_FRAGMENTS {
        return Err("prepared Ghost Talk carrier has an invalid physical fragment count".into());
    }
    payloads_hex
        .iter()
        .map(|payload_hex| {
            let payload = hex::decode(payload_hex)
                .map_err(|_| "prepared mailbox payload is not valid hex".to_string())?;
            if payload.is_empty()
                || payload.len() > ghost_core::MAX_KSPT_V1_PAYLOAD_BYTES
            {
                return Err(
                    "prepared Ghost Talk physical fragment exceeds the KSPT v1 payload boundary"
                        .into(),
                );
            }
            ghost_protocol::CarrierFrame::decode(&payload)?;
            Ok(payload)
        })
        .collect()
}

fn reassemble_mailbox_payloads(payloads: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if payloads.is_empty() || payloads.len() > ghost_core::MAX_FRAGMENTS {
        return Err("Ghost Talk carrier has an invalid fragment count".into());
    }
    let mut decoded = payloads
        .iter()
        .map(|payload| ghost_protocol::CarrierFrame::decode(payload))
        .collect::<Result<Vec<_>, _>>()?;
    let packet = decoded[0].packet;
    let count = decoded[0].count;
    if usize::from(count) != decoded.len()
        || decoded
            .iter()
            .any(|frame| frame.packet != packet || frame.count != count)
    {
        return Err("Ghost Talk carrier fragments do not form one complete packet".into());
    }
    decoded.sort_by_key(|frame| frame.index);
    for (expected, frame) in decoded.iter().enumerate() {
        if usize::from(frame.index) != expected {
            return Err("Ghost Talk carrier fragments are missing or duplicated".into());
        }
    }
    let total = decoded
        .iter()
        .try_fold(0usize, |total, frame| total.checked_add(frame.payload.len()))
        .ok_or_else(|| "Ghost Talk reassembled carrier length overflowed usize".to_string())?;
    let max_reassembled = ghost_core::MAX_FRAGMENTS.saturating_mul(ghost_protocol::GHST_DATA_MAX);
    if total > max_reassembled {
        return Err("Ghost Talk reassembled carrier exceeds the bounded GHST fragment window".into());
    }
    let mut carrier = Vec::with_capacity(total);
    for frame in decoded {
        carrier.extend_from_slice(&frame.payload);
    }
    Ok(carrier)
}

async fn wallet_utxo_total(
    portal: &PortalFacade,
    public: &WalletPublic,
) -> Result<u128, String> {
    let addresses = public.all_addresses().cloned().collect::<Vec<_>>();
    let utxos = portal.current_utxos(&addresses).await?;
    utxos.into_iter().try_fold(0u128, |total, utxo| {
        total
            .checked_add(u128::from(utxo.amount))
            .ok_or_else(|| "Ghost Talk wallet UTXO total overflowed u128".to_string())
    })
}

async fn wait_for_wallet_utxo_change(
    portal: &PortalFacade,
    public: &WalletPublic,
    previous_total: u128,
) -> Result<u128, String> {
    // Every serialized mailbox send waits until the accepted transaction has
    // changed the node's live wallet UTXO view before another send may plan.
    // Multi-fragment carriers use the same rule between fragments. This prevents
    // Auto signaling, voice control/media, and durable text from racing to spend
    // the same input while an earlier broadcast is still propagating.
    const ATTEMPTS: usize = 100;
    const DELAY: Duration = Duration::from_millis(100);
    for _ in 0..ATTEMPTS {
        tokio::time::sleep(DELAY).await;
        let current = wallet_utxo_total(portal, public).await?;
        if current < previous_total {
            return Ok(current);
        }
    }
    Err(
        "Kaspa accepted a Ghost Talk mailbox transaction, but the updated wallet UTXO set did not become visible before the next-send timeout; refresh wallet state before retrying"
            .into(),
    )
}

async fn send_mailbox_payloads_via_gateway(
    gateway: &super::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payloads: &[Vec<u8>],
    reuse_change: bool,
) -> Result<ghost_kaspa::wallet::BroadcastResult, String> {
    if payloads.is_empty() || payloads.len() > ghost_core::MAX_FRAGMENTS {
        return Err("Ghost Talk logical carrier has an invalid physical fragment count".into());
    }
    for payload in payloads {
        if payload.is_empty()
            || payload.len() > ghost_core::MAX_KSPT_V1_PAYLOAD_BYTES
        {
            return Err(
                "Ghost Talk attempted to sign a payload larger than KSPT v1 permits"
                    .into(),
            );
        }
    }

    // Capture the live total before the first plan. The caller holds the
    // profile's outbound lock for this entire function, so observing a lower
    // total after SubmitTransaction is an explicit handoff point to the next
    // mailbox send rather than a best-effort timing assumption.
    let mut visible_total = wallet_utxo_total(portal, public).await?;

    if payloads.len() == 1 {
        let result = if reuse_change {
            send_payload_reuse_change_via_gateway(
                gateway,
                portal,
                secret,
                public,
                destination,
                fee,
                &payloads[0],
            )
            .await?
        } else {
            send_payload_via_gateway(
                gateway,
                portal,
                secret,
                public,
                destination,
                fee,
                &payloads[0],
            )
            .await?
        };
        wait_for_wallet_utxo_change(portal, public, visible_total).await?;
        return Ok(result);
    }

    let mut total_fee = 0u128;
    let mut last_transaction_id = String::new();
    for (index, payload) in payloads.iter().enumerate() {
        if index > 0 {
            visible_total = wait_for_wallet_utxo_change(portal, public, visible_total).await?;
        }
        let result = send_payload_reuse_change_via_gateway(
            gateway,
            portal,
            secret,
            public,
            destination,
            fee,
            payload,
        )
        .await?;
        total_fee = total_fee
            .checked_add(
                result
                    .fee_sompi
                    .parse::<u128>()
                    .map_err(|_| "Portal returned a non-decimal fragment fee".to_string())?,
            )
            .ok_or_else(|| "Ghost Talk aggregate mailbox fee overflowed u128".to_string())?;
        last_transaction_id = result.transaction_id;
    }

    // The final accepted fragment must also become visible before the per-profile
    // outbound lock is released. Otherwise a following Auto/voice/text send can
    // immediately re-plan against the final fragment's already-spent input.
    wait_for_wallet_utxo_change(portal, public, visible_total).await?;

    let mut next_public = public.clone();
    if !reuse_change {
        next_public.advance_change()?;
    }
    Ok(ghost_kaspa::wallet::BroadcastResult {
        transaction_id: last_transaction_id,
        fee_sompi: total_fee.to_string(),
        public: next_public,
        timings: None,
    })
}

async fn send_payload_reuse_change_via_gateway(
    gateway: &super::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
) -> Result<ghost_kaspa::wallet::BroadcastResult, String> {
    let result = ghost_kaspa::wallet::send_payload_reuse_change(
        portal,
        secret,
        public,
        destination,
        fee,
        payload,
    )
    .await;
    if let Err(error) = &result {
        gateway.note_operation_error(error).await;
    }
    result
}

async fn send_payload_via_gateway(
    gateway: &super::kaspa_gateway::KaspaGatewayState,
    portal: &PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    fee: u64,
    payload: &[u8],
) -> Result<ghost_kaspa::wallet::BroadcastResult, String> {
    let result = ghost_kaspa::wallet::send_payload(
        portal,
        secret,
        public,
        destination,
        fee,
        payload,
    )
    .await;
    if let Err(error) = &result {
        // Never replay an ambiguous SubmitTransaction automatically. The single
        // gateway/block pump owns reconnect and failover for the one Portal; this
        // call only records the transport error for diagnostics.
        gateway.note_operation_error(error).await;
    }
    result
}

async fn start_live_block_stream(
    profile_id: &str,
    public: &WalletPublic,
    wrpc_override: Option<&str>,
    gateway: &super::kaspa_gateway::KaspaGatewayState,
) -> Result<LiveBlockStream, String> {
    let blocks = gateway.subscribe_blocks(public, wrpc_override).await?;
    crate::debug_log::record(
        "info",
        "mailbox",
        "block-added-subscribed",
        format!(
            "profile={} network={} gateway_fanout=true portal_rpc_plus_block_stream=true",
            profile_id, public.network
        ),
    );
    Ok(LiveBlockStream { blocks })
}

#[derive(Clone, Debug)]
struct PrivateMailboxSync {
    checkpoint: String,
    observations: Vec<MailboxEvent>,
}


async fn sync_mailbox(
    public: &WalletPublic,
    checkpoint: &str,
    directory_checkpoint: &str,
    rest_override: Option<&str>,
) -> Result<MailboxSyncResult, String> {
    let mailbox = sync_private_mailbox(public, checkpoint, rest_override).await?;
    Ok(MailboxSyncResult {
        checkpoint: mailbox.checkpoint,
        directory_checkpoint: directory_checkpoint.to_owned(),
        observations: mailbox.observations,
        public_profiles: Vec::new(),
    })
}

fn advance_mailbox_checkpoint(current: &mut String, candidate: &str) {
    let current_score = current.parse::<u64>().unwrap_or_default();
    let candidate_score = candidate.parse::<u64>().unwrap_or_default();
    if candidate_score >= current_score {
        *current = candidate.to_owned();
    }
}

async fn sync_priority_private_mailbox(
    public: &WalletPublic,
    checkpoint: &str,
    rest_override: Option<&str>,
) -> Result<PrivateMailboxSync, String> {
    let checkpoint = parse_checkpoint(checkpoint)?;
    let rest = rest_override
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| rest_base_for_network(&public.network));
    let history = RestHistory::new(rest, 4);
    let mut priority = Vec::<String>::new();
    for address in [
        public.receive_addresses.first(),
        public.receive_addresses.get(public.next_receive_index),
    ]
    .into_iter()
    .flatten()
    {
        if !priority.iter().any(|existing| existing == address) {
            priority.push(address.clone());
        }
    }
    if priority.is_empty() {
        return Ok(PrivateMailboxSync {
            checkpoint: checkpoint.to_string(),
            observations: Vec::new(),
        });
    }
    let after = checkpoint.saturating_sub(MAILBOX_HISTORY_OVERLAP);
    // Do not fetch the virtual tip before delivering priority traffic. The tip
    // endpoint is bookkeeping; making a signed Response wait on it recreated the
    // same head-of-line blocking this path exists to avoid. Broad catch-up advances
    // the durable tip separately. Priority only advances to an actually observed
    // carrier score and otherwise keeps its prior checkpoint.
    let observations = history
        .historical_mailbox_before_ms(&priority, after, historical_rest_cutoff_ms())
        .await?;
    let observed_checkpoint = observations
        .iter()
        .map(|observation| observation.blue_score)
        .max()
        .unwrap_or(checkpoint)
        .max(checkpoint);
    Ok(PrivateMailboxSync {
        checkpoint: observed_checkpoint.to_string(),
        observations: observations.into_iter().map(to_mailbox_event).collect(),
    })
}

async fn sync_private_mailbox(
    public: &WalletPublic,
    checkpoint: &str,
    rest_override: Option<&str>,
) -> Result<PrivateMailboxSync, String> {
    let checkpoint = parse_checkpoint(checkpoint)?;
    let rest = rest_override
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| rest_base_for_network(&public.network));
    let history = RestHistory::new(rest, 16);
    let watched = public.all_addresses().cloned().collect::<Vec<_>>();

    // REST is archival recovery only. Never use /addresses/active, REST virtual
    // tip state, or any transaction younger than 48 hours for the current
    // mailbox. Live/recent carriers belong to the sole Kaspa wRPC connection.
    let after = checkpoint.saturating_sub(MAILBOX_HISTORY_OVERLAP);
    let observations = history
        .historical_mailbox_before_ms(&watched, after, historical_rest_cutoff_ms())
        .await?;
    let observed_checkpoint = observations
        .iter()
        .map(|observation| observation.blue_score)
        .max()
        .unwrap_or(checkpoint)
        .max(checkpoint);
    let observations = observations.into_iter().map(to_mailbox_event).collect();

    Ok(PrivateMailboxSync {
        checkpoint: observed_checkpoint.to_string(),
        observations,
    })
}

fn live_public_profile_updates(
    observations: &[LiveTransactionObservation],
    tip: u64,
) -> Vec<super::peer_commands::PublicGhostProfile> {
    let mut latest = std::collections::BTreeMap::<String, (u64, super::peer_commands::PublicGhostProfile)>::new();
    for observation in observations {
        if !observation.payload.starts_with(&ghost_protocol::GTCD_MAGIC) {
            continue;
        }
        let Ok(descriptor) = ghost_protocol::GhostContactDescriptor::decode(&observation.payload) else {
            continue;
        };
        if descriptor.version != 1
            || descriptor.hydra_identity_id.len() != 64
            || !descriptor.hydra_identity_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || descriptor.expires_daa.is_some_and(|expires| tip != 0 && expires <= tip)
            || ghost_kaspa::verify_gtcd(&descriptor).is_err()
        {
            continue;
        }
        let profile = super::peer_commands::PublicGhostProfile {
            kaspa_address: descriptor.kaspa_address.clone(),
            display_name: descriptor.display_name,
            hydra_identity_id: descriptor.hydra_identity_id,
            descriptor_blue_score: observation.daa_score.to_string(),
            kns_name: None,
            username: descriptor.username,
            description: descriptor.description,
            interests: descriptor.interests,
            verified: true,
            discoverable: descriptor.discoverable,
        };
        match latest.get(&descriptor.kaspa_address) {
            Some((score, _)) if *score > observation.daa_score => {}
            _ => {
                latest.insert(descriptor.kaspa_address, (observation.daa_score, profile));
            }
        }
    }
    latest.into_values().map(|(_, profile)| profile).collect()
}


fn historical_rest_cutoff_ms() -> u64 {
    const FORTY_EIGHT_HOURS_MS: u64 = 48 * 60 * 60 * 1_000;
    observed_at_ms().saturating_sub(FORTY_EIGHT_HOURS_MS)
}

fn observed_at_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn to_mailbox_event(observation: DagObservation) -> MailboxEvent {
    MailboxEvent {
        transaction_id: observation.transaction_id,
        blue_score: observation.blue_score.to_string(),
        payload_hex: hex::encode(observation.payload),
        block_time: observation.block_time,
    }
}

fn to_live_mailbox_event(observation: LiveTransactionObservation) -> MailboxEvent {
    MailboxEvent {
        transaction_id: observation.txid,
        blue_score: observation.daa_score.to_string(),
        payload_hex: hex::encode(observation.payload),
        block_time: None,
    }
}

fn parse_checkpoint(value: &str) -> Result<u64, String> {
    if value.is_empty() {
        return Ok(0);
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("mailbox checkpoint must be a decimal integer".into());
    }
    value
        .parse::<u64>()
        .map_err(|_| "mailbox checkpoint exceeds u64".to_string())
}
