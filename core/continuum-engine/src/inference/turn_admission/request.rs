//! Request deadlines and the public entry point for the shared turn admission owner.

use super::*;

/// Independent admission budgets: readiness waits consume `readiness`; every wait
/// consumes the request's absolute deadline when it has one.
pub struct AdmissionPatience {
    pub(super) readiness: std::time::Duration,
    deadline: Option<tokio::time::Instant>,
}

impl AdmissionPatience {
    /// A generation occupancy estimate is not an admission expiry. Healthy queue
    /// and paging work stays owned until admitted or the requester cancels.
    pub fn for_turn() -> Self {
        Self {
            readiness: crate::inference::slots::EndpointSlots::TURN_READINESS_PATIENCE,
            deadline: None,
        }
    }

    /// An explicit caller expiry, already expressed on the monotonic clock.
    /// Do not derive this from predicted generation occupancy.
    pub fn until(deadline: tokio::time::Instant) -> Self {
        Self { deadline: Some(deadline), ..Self::for_turn() }
    }
}

impl From<std::time::Duration> for AdmissionPatience {
    fn from(readiness: std::time::Duration) -> Self {
        Self { readiness, deadline: None }
    }
}

/// Admit a turn: wait for the mind's running act (a keyed turn only; see
/// `slots::mind_gate`), acquire the concurrency permit (always), then — for a turn with a
/// [`PageKey`] and a live slot pool — lease + pin its slot and page its KV (save the
/// evictee, restore this mind's page). Returns the guard the
/// caller must hold across the generation.
///
/// `key`/`pool` are `None` for non-Turn traffic (or a cloud provider with no slots):
/// the caller still gets the permit and places the request on scratch (or unpinned)
/// itself. On a single-slot server the pool saves and detaches the resident before
/// transient traffic borrows that slot; `slot()` then returns its index.
///
/// A keyed dream's refusal when her act is running or queued: her act comes first, and the
/// dream (her learning step) is tried again later, having held nothing.
pub const HER_ACT_FIRST: &str = "her act comes first: her own background work yields to it";

/// `patience` is how long the turn holds through a CLOSED endpoint (an engine still
/// loading, a transition in flight) before it is refused — the caller's bound, since
/// only the caller knows what its request can afford: the persona adapter passes
/// `AdmissionPatience::for_turn()`. Readiness time is cumulative across retries;
/// healthy slot and adapter waits end only on cancellation or an explicit caller
/// expiry supplied with `AdmissionPatience::until`.
/// A test that asserts immediate readiness refusal passes zero. See `EndpointSlots::admit_when_ready`
/// for the boot window that cost Kimi 16 turns.
pub async fn admit_turn(
    concurrency: impl Into<DecodeBound>,
    class: SlotClass,
    key: Option<PageKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
    patience: impl Into<AdmissionPatience>,
) -> Result<TurnAdmission, String> {
    admit_prepared_turn(concurrency, class, key, pool, client, root, approx_tokens, patience, None)
        .await.map_err(AdmissionFailure::into_text)
}
pub(crate) async fn admit_prepared_turn(
    concurrency: impl Into<DecodeBound>,
    class: SlotClass,
    key: Option<PageKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
    patience: impl Into<AdmissionPatience>,
    preparation: Option<Preparation>,
) -> Result<TurnAdmission, AdmissionFailure> {
    let patience = patience.into();
    let concurrency = concurrency.into();
    let admission = admit(&concurrency, class, key, pool, client, root, approx_tokens, false, patience.readiness, preparation);
    match patience.deadline {
        Some(deadline) => {
            if deadline <= tokio::time::Instant::now() {
                return Err("turn deadline elapsed before admission".to_owned().into());
            }
            tokio::time::timeout_at(deadline, admission).await
                .map_err(|_| "turn deadline elapsed during admission".to_owned())?
        }
        None => admission.await,
    }
}
