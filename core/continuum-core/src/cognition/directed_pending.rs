//! A directed line is pending for a citizen: the signal that lets her yield a
//! parked SELF-work lane wait to answer it.
//!
//! Measured 2026-09-04 (the deaf-citizens night): inbound is drained only at
//! the service loop's head, and a self-tick holds the loop for a whole work
//! turn — most of which, at 12 citizens on 5 lanes, is spent PARKED at the
//! non-directed lane gate. A human line therefore waited out the park plus the
//! turn (~10 min) before it could even be heard. The pump raises this flag when
//! a line from outside the citizenry (or one that names her) is forwarded; the
//! deliberation faculty, while parked for a NON-directed lane, yields on it
//! (the wait was free to abandon — no lane was held); the loop head clears it
//! when it drains. A directed turn itself never yields.
//!
//! One concern, one file, no tokio task: a flag + a `Notify` per citizen.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use tokio::sync::Notify;
use uuid::Uuid;

struct Pending {
    flag: AtomicBool,
    notify: Notify,
}

static PENDING: LazyLock<Mutex<HashMap<Uuid, Arc<Pending>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cell(persona: Uuid) -> Arc<Pending> {
    let mut map = PENDING.lock().unwrap_or_else(|e| e.into_inner());  // poisoned lock = read the last state, same policy as every lock in this crate
    Arc::clone(map.entry(persona).or_insert_with(|| {
        Arc::new(Pending { flag: AtomicBool::new(false), notify: Notify::new() })
    }))
}

/// A directed line was forwarded to this citizen's inbox.
pub fn signal(persona: Uuid) {
    let c = cell(persona);
    c.flag.store(true, Ordering::SeqCst);
    c.notify.notify_waiters();
}

/// The loop head drained the inbox: whatever was pending is now in hand.
pub fn clear(persona: Uuid) {
    cell(persona).flag.store(false, Ordering::SeqCst);
}

/// Is a directed line pending right now (no wait)?
pub fn is_pending(persona: Uuid) -> bool {
    cell(persona).flag.load(Ordering::SeqCst)
}

/// Resolve when a directed line is pending — immediately if one already is.
pub async fn wait(persona: Uuid) {
    let c = cell(persona);
    loop {
        // Register interest BEFORE the flag check so a signal between the
        // check and the await is not lost (`notify_waiters` wakes only
        // registered waiters).
        let notified = c.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if c.flag.load(Ordering::SeqCst) {
            return;
        }
        notified.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a line that arrived BEFORE the wait began must still
    // preempt (the flag, not only the wake, carries the fact).
    #[tokio::test]
    async fn a_signal_before_the_wait_resolves_immediately() {
        let p = Uuid::new_v4();
        signal(p);
        tokio::time::timeout(std::time::Duration::from_millis(50), wait(p))
            .await
            .expect("pending flag must resolve the wait at once");
        clear(p);
        assert!(!is_pending(p));
    }

    // what this catches: a wait parked first must wake on the signal, and a
    // cleared flag must park again (no stale preemption of the next turn).
    #[tokio::test]
    async fn a_signal_wakes_a_parked_wait_and_clear_parks_the_next() {
        let p = Uuid::new_v4();
        let waiter = tokio::spawn(wait(p));
        tokio::task::yield_now().await;
        signal(p);
        tokio::time::timeout(std::time::Duration::from_millis(200), waiter)
            .await
            .expect("signal must wake the parked wait")
            .unwrap();
        clear(p);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), wait(p)).await.is_err(),
            "a cleared flag parks the next wait"
        );
    }
}
