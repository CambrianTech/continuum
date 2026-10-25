//! A caller owns its deadline; the endpoint owns an already-dispatched page IO.
//! Only physical-slot holders enter here. There is no generation or admission
//! queue in the retained task, and the existing paging watchdog is its bound.

use super::TurnAdmission;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Interest {
    active: Arc<AtomicBool>,
    root: String,
    waiting: bool,
}

impl Drop for Interest {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        if self.waiting {
            continuum_base::probe!(class = "inference.kv_page.waiter_left", endpoint = %self.root,
                "requester left; endpoint retains paging and its guards until terminal receipt");
        }
    }
}

/// If the waiter vanishes before accepting the result, a verified restore still
/// belongs to its activity. No generation has run, so preserve that physical fact.
/// Once taken, ordinary TurnAdmission cancellation rules apply again.
struct Handoff(Option<TurnAdmission>);

impl Drop for Handoff {
    fn drop(&mut self) {
        if let Some(admission) = &mut self.0 {
            if admission.page_confirmed {
                admission.uncommitted = None;
            }
        }
    }
}

impl TurnAdmission {
    pub(super) async fn retain_paging<F, Fut>(
        self,
        root: &str,
        operation: F,
    ) -> Result<Self, String>
    where
        F: FnOnce(Self, Arc<AtomicBool>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Self, String>> + Send + 'static,
    {
        let interested = Arc::new(AtomicBool::new(true));
        let mut interest = Interest {
            active: interested.clone(),
            root: root.to_owned(),
            waiting: true,
        };
        let (send, receive) = tokio::sync::oneshot::channel();
        crate::inference::slots::directory()
            .endpoint(root)
            .retain_paging(async move {
                // The requester may have left before this task was first polled.
                if !interested.load(Ordering::Acquire) {
                    return;
                }
                let result = operation(self, interested)
                    .await
                    .map(|admission| Handoff(Some(admission)));
                // Failed delivery drops Handoff here, after the terminal receipt and
                // ledger updates, rather than cancelling the IO or releasing early.
                let _ = send.send(result);
            });
        let result = receive.await;
        interest.waiting = false;
        let mut handoff =
            result.map_err(|_| "endpoint paging owner ended without a result".to_owned())??;
        handoff
            .0
            .take()
            .ok_or_else(|| "endpoint paging handoff already consumed".to_owned())
    }
}
