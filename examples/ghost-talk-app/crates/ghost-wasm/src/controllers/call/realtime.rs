use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_p2p::{GhostP2pTransport, P2pNetTransport, P2pRouteState};
use ghost_realtime::{route, RealtimeCarrier, RoutePreference};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::{Callback, UseStateHandle};

use crate::{model::Profile, random_id};

#[derive(Clone)]
pub(crate) struct RealtimeSender {
    inner: Rc<RefCell<RealtimeSenderInner>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RealtimeDelivery {
    Realtime,
    KaspaRealtime,
    DurableControl,
}

#[derive(Clone)]
struct RealtimeJob {
    chat_id: String,
    body: String,
    delivery: RealtimeDelivery,
    on_complete: Option<Callback<Result<(), String>>>,
}

struct RealtimeSenderInner {
    profile: Profile,
    password: String,
    queue: VecDeque<RealtimeJob>,
    running: bool,
    p2p: Rc<RefCell<P2pNetTransport>>,
    p2p_state: UseStateHandle<P2pRouteState>,
    on_wallet_progress: Callback<crate::model::WalletProjection>,
    on_error: Callback<String>,
}

fn realtime_target(
    profile: &Profile,
    job: &RealtimeJob,
) -> Result<(String, String, String), String> {
    let chat = profile
        .chats
        .iter()
        .find(|chat| chat.id == job.chat_id)
        .ok_or_else(|| "Realtime chat no longer exists.".to_string())?;
    let peer = chat
        .peer_hydra_handle()
        .map(str::to_owned)
        .ok_or_else(|| "Realtime chat has no authenticated HYDRA peer.".to_string())?;
    let destination = chat
        .peer_kaspa_address()
        .map(str::to_owned)
        .ok_or_else(|| "Realtime chat has no Kaspa destination.".to_string())?;
    Ok((peer, destination, random_id()?))
}

async fn send_realtime_job(
    snapshot: &SenderSnapshot,
    job: &RealtimeJob,
    target: &(String, String, String),
) -> Result<Option<ghost_api::MailboxSendResult>, String> {
    let (peer, destination, message_id) = target;
    if job.delivery == RealtimeDelivery::DurableControl {
        return crate::controllers::call::send_call_control_message(
            &snapshot.profile,
            &snapshot.password,
            peer,
            destination,
            &job.body,
            message_id,
        )
        .await
        .map(Some);
    }

    // Advance HYDRA exactly once. Every physical carrier gets these exact GTR1
    // bytes, so p2p failure cannot cause a second ratchet send or a second
    // ciphertext for the same logical realtime message.
    let sealed = crate::controllers::call::seal_realtime(
        &snapshot.profile.id,
        peer,
        message_id,
        &job.body,
    )
    .await?;

    let preference = if job.delivery == RealtimeDelivery::Realtime
        && snapshot.profile.settings.route.eq_ignore_ascii_case("auto")
    {
        RoutePreference::Auto
    } else {
        RoutePreference::KaspaOnly
    };
    let decision = route(preference, *snapshot.p2p_state == P2pRouteState::Connected);

    if decision.primary == RealtimeCarrier::P2pNet {
        let sid = decode_hex_array::<16>(&sealed.session_sid, "GTR1 SID")?;
        let packet = STANDARD
            .decode(sealed.carrier_b64.as_bytes())
            .map_err(|_| "sealed GTR1 carrier is not valid base64".to_string())?;
        let p2p_result = match snapshot.p2p.try_borrow() {
            Ok(p2p) => p2p.send(sid, &packet).await,
            Err(_) => Err("p2p-net is busy with another lifecycle operation".into()),
        };
        if p2p_result.is_ok() {
            return Ok(None);
        }
        snapshot.p2p_state.set(P2pRouteState::KaspaFallback);
        if decision.fallback != Some(RealtimeCarrier::Kaspa) {
            return p2p_result.map(|_| None);
        }
    }

    crate::controllers::call::send_realtime_carrier(
        &snapshot.profile,
        &snapshot.password,
        peer,
        destination,
        &sealed.carrier_b64,
    )
    .await
    .map(Some)
}

impl RealtimeSender {
    pub(crate) fn new(
        profile: Profile,
        password: String,
        p2p: Rc<RefCell<P2pNetTransport>>,
        p2p_state: UseStateHandle<P2pRouteState>,
        on_wallet_progress: Callback<crate::model::WalletProjection>,
        on_error: Callback<String>,
    ) -> Self {
        Self {
            inner: Rc::new(RefCell::new(RealtimeSenderInner {
                profile,
                password,
                queue: VecDeque::new(),
                running: false,
                p2p,
                p2p_state,
                on_wallet_progress,
                on_error,
            })),
        }
    }

    pub(crate) fn sync(
        &self,
        profile: Profile,
        password: String,
        on_wallet_progress: Callback<crate::model::WalletProjection>,
        on_error: Callback<String>,
    ) {
        let mut inner = self.inner.borrow_mut();
        if profile.state_revision() > inner.profile.state_revision()
            || profile.id != inner.profile.id
        {
            inner.profile = profile;
        }
        inner.password = password;
        inner.on_wallet_progress = on_wallet_progress;
        inner.on_error = on_error;
    }

    pub(crate) fn enqueue(&self, chat_id: String, body: String) {
        self.enqueue_job(RealtimeJob {
            chat_id,
            body,
            delivery: RealtimeDelivery::Realtime,
            on_complete: None,
        });
    }

    /// Queue a realtime GTR1 body that must travel over Kaspa. This is used for
    /// authenticated p2p binding announcements/acks so p2p never bootstraps its
    /// own identity assertion.
    pub(crate) fn enqueue_kaspa(&self, chat_id: String, body: String) {
        self.enqueue_job(RealtimeJob {
            chat_id,
            body,
            delivery: RealtimeDelivery::KaspaRealtime,
            on_complete: None,
        });
    }

    pub(crate) fn enqueue_control(
        &self,
        chat_id: String,
        body: String,
        on_complete: Option<Callback<Result<(), String>>>,
    ) {
        self.enqueue_job(RealtimeJob {
            chat_id,
            body,
            delivery: RealtimeDelivery::DurableControl,
            on_complete,
        });
    }

    pub(crate) fn discard_realtime_for_chat(&self, chat_id: &str) {
        self.inner.borrow_mut().queue.retain(|job| {
            job.delivery == RealtimeDelivery::DurableControl || job.chat_id != chat_id
        });
    }

    fn enqueue_job(&self, job: RealtimeJob) {
        let should_start = {
            let mut inner = self.inner.borrow_mut();
            if job.delivery == RealtimeDelivery::DurableControl {
                let insert_at = inner
                    .queue
                    .iter()
                    .position(|queued| queued.delivery != RealtimeDelivery::DurableControl)
                    .unwrap_or(inner.queue.len());
                inner.queue.insert(insert_at, job);
            } else {
                inner.queue.push_back(job);
            }
            if inner.running {
                false
            } else {
                inner.running = true;
                true
            }
        };
        if should_start {
            let sender = self.clone();
            spawn_local(async move { sender.drain().await });
        }
    }

    async fn drain(self) {
        loop {
            let Some(job) = self.inner.borrow_mut().queue.pop_front() else {
                self.inner.borrow_mut().running = false;
                return;
            };
            let snapshot = self.snapshot();
            let target = match realtime_target(&snapshot.profile, &job) {
                Ok(target) => target,
                Err(error) => {
                    self.finish_error(job, snapshot.on_error, error);
                    continue;
                }
            };
            match send_realtime_job(&snapshot, &job, &target).await {
                Ok(sent) => self.finish_success(job, sent, snapshot.on_wallet_progress),
                Err(error) => self.finish_error(job, snapshot.on_error, error),
            }
        }
    }

    fn snapshot(&self) -> SenderSnapshot {
        let inner = self.inner.borrow();
        SenderSnapshot {
            profile: inner.profile.clone(),
            password: inner.password.clone(),
            p2p: inner.p2p.clone(),
            p2p_state: inner.p2p_state.clone(),
            on_wallet_progress: inner.on_wallet_progress.clone(),
            on_error: inner.on_error.clone(),
        }
    }

    fn finish_success(
        &self,
        job: RealtimeJob,
        sent: Option<ghost_api::MailboxSendResult>,
        on_wallet_progress: Callback<crate::model::WalletProjection>,
    ) {
        if let Some(sent) = sent {
            let public = sent.public;
            {
                let mut inner = self.inner.borrow_mut();
                crate::model::WalletStateService::merge_progress(
                    &mut inner.profile.wallet,
                    public.clone(),
                );
            }
            // Kaspa sends can advance wallet state. A p2p-only send has no
            // wallet projection and must not manufacture one.
            on_wallet_progress.emit(public);
        }
        if let Some(callback) = job.on_complete {
            callback.emit(Ok(()));
        }
    }

    fn finish_error(&self, job: RealtimeJob, on_error: Callback<String>, error: String) {
        if let Some(callback) = job.on_complete {
            callback.emit(Err(error.clone()));
        }
        if job.delivery == RealtimeDelivery::DurableControl {
            on_error.emit(format!("Call signaling failed: {error}"));
        } else {
            on_error.emit(error);
        }
    }
}

struct SenderSnapshot {
    profile: Profile,
    password: String,
    p2p: Rc<RefCell<P2pNetTransport>>,
    p2p_state: UseStateHandle<P2pRouteState>,
    on_wallet_progress: Callback<crate::model::WalletProjection>,
    on_error: Callback<String>,
}

fn decode_hex_array<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} is not valid hex"));
    }
    let decoded = hex::decode(value).map_err(|_| format!("{label} is not valid hex"))?;
    decoded
        .try_into()
        .map_err(|_| format!("{label} has an invalid length"))
}
