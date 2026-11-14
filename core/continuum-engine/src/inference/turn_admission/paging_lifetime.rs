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

/// The page IO's result travels to its requester as a handoff (`runtime::stage::handoff`): if
/// the requester vanishes first, a verified restore still belongs to its activity, so the
/// admission is dropped with the delivery after the terminal receipt and ledger updates, and its
/// own drop keeps the residency (nothing was sent: `Dispatch::Unsent`).
const PAGE_IO_RESULT: crate::runtime::stage::TopicName =
    crate::runtime::stage::TopicName("inference/kv_page.result");

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
        use crate::runtime::stage::{handoff, Settled};
        let interested = Arc::new(AtomicBool::new(true));
        let mut interest = Interest {
            active: interested.clone(),
            root: root.to_owned(),
            waiting: true,
        };
        let (delivers, result) = handoff(PAGE_IO_RESULT);
        crate::inference::slots::directory()
            .endpoint(root)
            .retain_paging(async move {
                // The requester may have left before this task was first polled.
                if !interested.load(Ordering::Acquire) {
                    return;
                }
                match operation(self, interested).await {
                    Ok(admission) => delivers.hand_over(admission),
                    Err(reason) => delivers.refuse(reason),
                }
            });
        let settled = result.settled().await.settled;
        interest.waiting = false;
        match settled {
            Settled::Ready(admission) => Ok(admission),
            Settled::Failed(reason) => Err(reason),
            Settled::Abandoned => Err("endpoint paging owner ended without a result".to_owned()),
        }
    }
}
