use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use ghost_p2p::{P2pNetTransport, P2pRouteState};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::{Callback, UseStateHandle};

use crate::model::Profile;

mod job;

#[derive(Clone)]
pub(crate) struct RealtimeSender {
    inner: Rc<RefCell<RealtimeSenderInner>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RealtimeDelivery {
    /// Live media: p2p-net when routed Automatic, else Kaspa.
    Realtime,
    /// Must travel over Kaspa (authenticated p2p binding announcements).
    KaspaRealtime,
    /// Ephemeral chat text: p2p-net only, never stored on Kaspa.
    DirectOnly,
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
        p2p_state: UseStateHandle<P2pRouteState>,
        on_wallet_progress: Callback<crate::model::WalletProjection>,
        on_error: Callback<String>,
    ) {
        let mut inner = self.inner.borrow_mut();
        // A `UseStateHandle` reads the value of the render it came from, so
        // take this render's handle: route decisions must see the live state.
        inner.p2p_state = p2p_state;
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

    /// Queue ephemeral chat text that may only travel over p2p-net.
    pub(crate) fn enqueue_direct(
        &self,
        chat_id: String,
        body: String,
        on_complete: Callback<Result<(), String>>,
    ) {
        self.enqueue_job(RealtimeJob {
            chat_id,
            body,
            delivery: RealtimeDelivery::DirectOnly,
            on_complete: Some(on_complete),
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
            let target = match job::target(&snapshot.profile, &job) {
                Ok(target) => target,
                Err(error) => {
                    self.finish_error(job, snapshot.on_error, error);
                    continue;
                }
            };
            match job::send(&snapshot, &job, &target).await {
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
        match job.delivery {
            // The composer reports direct-text failures (and may fall back).
            RealtimeDelivery::DirectOnly => {}
            RealtimeDelivery::DurableControl => {
                on_error.emit(format!("Call signaling failed: {error}"))
            }
            _ => on_error.emit(error),
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
