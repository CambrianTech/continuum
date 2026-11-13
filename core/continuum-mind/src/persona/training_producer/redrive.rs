//! Coalesced re-drive of held reviews (card fe8554d2). Staging a row that carries a pending
//! submission's generation asks for one pass of `reconcile_reviews` (idempotent: an accepted
//! binding returns Accepted, a held one holds). Repeated staging must not stack passes: one runs
//! per persona at a time, and a request that lands while it runs queues exactly one more pass,
//! so a row staged mid-pass is still seen.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use uuid::Uuid;

/// Persona -> "another pass was requested while this one ran".
static RUNNING: LazyLock<Mutex<HashMap<Uuid, bool>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn request(persona: Uuid, name: String, runtime: Arc<crate::persona::PersonaAircRuntime>) {
    let Ok(mut running) = RUNNING.lock() else {
        return; // poisoned by a panicked pass: her next registration replays every review
    };
    if let Some(again) = running.get_mut(&persona) {
        *again = true;
        return;
    }
    running.insert(persona, false);
    drop(running);
    tokio::spawn(async move {
        loop {
            super::reconcile_reviews(&name, runtime.airc()).await;
            let Ok(mut running) = RUNNING.lock() else { return };
            match running.get_mut(&persona) {
                Some(again) if *again => *again = false,
                _ => {
                    running.remove(&persona);
                    return;
                }
            }
        }
    });
}
