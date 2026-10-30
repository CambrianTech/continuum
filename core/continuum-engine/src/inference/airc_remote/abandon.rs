//! A requester that abandons a remote generation tells the seat to stop (card 83f774fb).
//!
//! IntelMac, 2026-10-23: Iris gave up on the M5 at her deadline and the M5 kept going,
//! finishing 2,520 tokens ~55 s later for no one and holding a lane the whole time. The
//! seat (`ipc::command_ingress`) aborts a served generation on `ai/generate/cancel` from
//! its requester; this guard sends that cancel on every path that abandons the wait.

use std::sync::Arc;

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::Airc;
use continuum_airc_protocol::{
    AircCommandRequest, GenerateCancel, COMMAND_PATH_AI_GENERATE_CANCEL, DEFAULT_COMMAND_DEADLINE,
    KIND_PEER,
};
use uuid::Uuid;

use crate::routing::airc_transport::AircTransport;

/// Armed BEFORE the request is published (airc publishes before `request` returns),
/// keyed by the requester's own generation id. Dropped while armed (a deadline, any error,
/// or the turn dropping its future, even mid-publication) it fires its action, which sends
/// the seat a cancel. Only [`AbandonGuard::disarm`] (a reply arrived) stops it: no error
/// proves a request was not published (a publish can fail after the daemon accepted it),
/// and a cancel for work that never arrived is harmless.
pub(super) struct AbandonGuard {
    armed: Option<Box<dyn FnOnce() + Send>>,
}

struct Cancel {
    airc: Arc<Airc>,
    seat: PeerId,
    generation: Uuid,
    valid_until_ms: u64,
}

impl AbandonGuard {
    /// Cancellation is mandatory for every remote generation.
    /// `valid_until_ms` is the request's own stamped validity, carried on the cancel.
    pub(super) fn new(
        airc: Arc<Airc>,
        seat: PeerId,
        generation: Uuid,
        valid_until_ms: u64,
    ) -> Self {
        let cancel = Cancel {
            airc,
            seat,
            generation,
            valid_until_ms,
        };
        Self {
            armed: Some(Box::new(move || cancel.dispatch())),
        }
    }

    /// A guard whose abandonment runs `action` instead of a wire cancel (tests).
    #[cfg(test)]
    fn with_action(action: impl FnOnce() + Send + 'static) -> Self {
        Self {
            armed: Some(Box::new(action)),
        }
    }

    /// A reply arrived: nothing to cancel.
    pub(super) fn disarm(mut self) {
        self.armed = None;
    }
}

impl Drop for AbandonGuard {
    fn drop(&mut self) {
        if let Some(action) = self.armed.take() {
            action();
        }
    }
}

/// Publish with `guard` already armed, and hand it back with the pending request. If this
/// future is dropped mid-publication, or the publication fails in any way, the guard drops
/// armed and the seat is told to stop: neither case proves the request never reached it.
pub(super) async fn publish_armed<T, E>(
    guard: AbandonGuard,
    publish: impl std::future::Future<Output = Result<T, E>>,
) -> Result<(T, AbandonGuard), E> {
    let published = publish.await?;
    Ok((published, guard))
}

impl Cancel {
    /// Drop cannot await: the cancel rides its own task. Outside a runtime (process
    /// teardown) there is nothing to send it on; the seat's own deadline still ends it.
    fn dispatch(self) {
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(self.send());
            }
            Err(_) => continuum_base::probe!(
                class = "remote_lane.cancel_unsent",
                peer = %self.seat.0,
                generation = %self.generation,
                "abandoned a remote generation outside a runtime; the seat was not told"
            ),
        }
    }

    async fn send(self) {
        let request = AircCommandRequest {
            path: COMMAND_PATH_AI_GENERATE_CANCEL.to_string(),
            kind: KIND_PEER.to_string(),
            env: None,
            params: serde_json::json!(GenerateCancel {
                generation: self.generation,
                valid_until_ms: self.valid_until_ms,
            }),
        };
        let headers = AircTransport::build_headers(&request);
        let body = serde_json::to_value(&request); // boundary: the airc wire, this request's body
        let sent = match body {
            Ok(body) => self
                .airc
                .request(
                    MentionTarget::Peer(self.seat),
                    headers,
                    Body::Json(body),
                    DEFAULT_COMMAND_DEADLINE,
                )
                .await
                .map(|_pending| ())
                .map_err(|e| e.to_string()),
            Err(e) => Err(format!("serialize cancel: {e}")),
        };
        // The seat's reply (cancelled / not_serving / not_yours) is its own receipt; the
        // requester has already given up and does not wait for it.
        match sent {
            Ok(()) => continuum_base::probe!(
                class = "remote_lane.cancel_sent",
                peer = %self.seat.0,
                generation = %self.generation,
                "abandoned a remote generation; told the seat to stop"
            ),
            Err(error) => continuum_base::probe!(
                class = "remote_lane.cancel_unsent",
                peer = %self.seat.0,
                generation = %self.generation,
                error = %error,
                "abandoned a remote generation; the cancel could not be sent"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fired() -> (std::sync::Arc<std::sync::atomic::AtomicBool>, AbandonGuard) {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let set = std::sync::Arc::clone(&flag);
        let guard =
            AbandonGuard::with_action(move || set.store(true, std::sync::atomic::Ordering::SeqCst));
        (flag, guard)
    }

    // what this catches (Codex review of cf08ff017): the window between airc publishing
    // a request and request returning. The guard is armed before publication, so a
    // turn dropped while publication is still pending fires the cancel, and a publication
    // that FAILS fires it too (an error does not prove the frame was never accepted).
    // Only a reply (disarm) stops it.
    #[tokio::test]
    async fn the_guard_fires_through_the_publication_window_and_on_any_failure() {
        let (flag, guard) = fired();
        let accepted = std::sync::Arc::new(tokio::sync::Notify::new());
        let marker = std::sync::Arc::clone(&accepted);
        // The daemon accepts the frame, but request has not returned to us yet.
        let mut publishing = Box::pin(publish_armed(guard, async move {
            marker.notify_one();
            std::future::pending::<Result<(), ()>>().await
        }));
        assert!(
            futures::poll!(&mut publishing).is_pending(),
            "polled into the publication"
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), accepted.notified())
            .await
            .expect("the publication was accepted before its return");
        assert!(
            !flag.load(std::sync::atomic::Ordering::SeqCst),
            "still waiting: not fired yet"
        );
        drop(publishing);
        assert!(
            flag.load(std::sync::atomic::Ordering::SeqCst),
            "dropped after acceptance, before request returned: cancel sent"
        );

        let (flag, guard) = fired();
        let failed = publish_armed(guard, async { Err::<(), &str>("publish ack lost") }).await;
        assert!(failed.is_err());
        assert!(
            flag.load(std::sync::atomic::Ordering::SeqCst),
            "an ambiguous publish error: cancel sent"
        );

        let (flag, guard) = fired();
        let (_, guard) = publish_armed(guard, async { Ok::<(), ()>(()) })
            .await
            .expect("published");
        assert!(
            !flag.load(std::sync::atomic::Ordering::SeqCst),
            "published and still waiting: not fired"
        );
        guard.disarm();
        assert!(
            !flag.load(std::sync::atomic::Ordering::SeqCst),
            "a reply disarms it"
        );
    }
}
