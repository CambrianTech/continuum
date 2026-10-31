//! Snapshot responsibility of the llama-server process owner.
use super::*;
/// Published serving state. One model, is it ready, on what `/v1` url. The
/// daemon owns the `watch::Sender<ServingSnapshot>`; everything downstream
/// (adapters, operators, a future grid allocator) reads it instead of probing
/// the process directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ServingSnapshot.ts"
)]
pub struct ServingSnapshot {
    /// The model id currently being served, if any. `None` = nothing live yet.
    #[ts(optional)]
    pub active_model: Option<String>,
    /// True once `/health` has answered 200 for the active model.
    ///
    /// READ THIS WITH [`ready_verified_at_ms`](Self::ready_verified_at_ms). `ready`
    /// is a CACHED CLAIM, not a live probe (this snapshot is a `watch` borrow by
    /// design — see `serving/status`). Nothing revises it when the process dies, so
    /// on 2026-08-05 `serving/status` returned `ready: true, degraded_reason: null`
    /// AFTER the llama-server was SIGKILLed and its port was dead. Every consumer
    /// that trusted the bare bool was reading a claim with no expiry — the same
    /// defect class as an `[ok]` route health on a route that has not delivered in
    /// ten hours. A claim must carry the age of its evidence.
    pub ready: bool,
    /// When `ready` was last CONFIRMED by real evidence (a `/health` 200 or a real
    /// token delivery) — epoch ms. `None` = never confirmed.
    ///
    /// This is the expiry the bare `ready` bool lacks. A reader deciding anything
    /// load-bearing (route a persona here? score a benchmark against it?) must ask
    /// how old the confirmation is, not merely whether the flag is set: `ready:true`
    /// verified 3s ago and `ready:true` verified 40 minutes ago are different facts,
    /// and only one of them is a lane you should send work to.
    /// [[a-wedged-llama-slot-spins-forever-while-health-and-serving-status-both-say-ready]]
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub ready_verified_at_ms: Option<u64>,
    /// The `/v1` base url personas point their inference adapter at.
    pub base_url: String,
    /// The LoRA genome layers loaded into the serving catalog (sorted paths).
    /// Empty = base model only. Lets a reader (and the reconcile guard) see WHICH
    /// genome is live without probing the process. `serde(default)` keeps older
    /// persisted snapshots readable.
    #[serde(default)]
    pub adapters: Vec<String>,
    /// The REAL per-slot context window the running server serves, read from its
    /// own `/props` (`default_generation_settings.n_ctx`). This is the
    /// AUTHORITATIVE model metadata personas budget their prompts to. llama.cpp
    /// pads the launch per-slot window (`-c / --parallel`) UP to a 256-multiple
    /// internally, so the planner's window and the served window differ — and the
    /// planner RE-computes its window every tick against live memory, drifting
    /// ABOVE the running server's frozen slot. Budgeting to that drifted value
    /// overflows the slot → llama-server 500 "Compute error". So a persona reads
    /// THIS (the process's own truth), never a recomputed plan value. `0` only on
    /// the empty/not-yet-served snapshot — a `ready` snapshot always carries the
    /// real window (the daemon refuses to publish `ready` without it).
    /// `serde(default)` keeps older persisted snapshots (window-less) readable.
    #[serde(default)]
    pub served_context_window: u32,
    /// The `--parallel` slot count the running server serves — how many personas
    /// can occupy a lane concurrently. llama.cpp allocates one full
    /// `served_context_window` KV window PER slot, so total resident KV scales
    /// with this: `lanes × kv_per_token × served_context_window`. Carried on the
    /// snapshot so a reader (the resource authority's `footprint()`, a grid
    /// allocator sizing concurrency) sees the true residency without probing the
    /// process. `0` only on the empty/not-yet-served snapshot — a `ready`
    /// snapshot always carries the real lane count. `serde(default)` keeps older
    /// persisted snapshots (lane-less) readable.
    #[serde(default)]
    pub lanes: u32,
    /// The host prompt cache (`--cache-ram`, MiB) the running engine was LAUNCHED
    /// with — a fact about the process, recorded at spawn, never the plan's latest
    /// derivation. The footprint measurement subtracts it before attributing bytes
    /// per token and the board credits it to serving; both must charge what the
    /// ENGINE holds. Measured 2026-09-20 07:52Z on the M5: the daemon re-derived a
    /// 1-lane target (256 MiB) while the engine still ran 2 lanes on a 14,396 MiB
    /// grant; the subtraction used the new number, the per-token reading went
    /// 33k → 262k, the prefill "spike" 2.8 → 16.6 GB, the plan to 1 × 2,048, and
    /// the record on disk kept it for the next boot. For a lane this core did not
    /// spawn (ADOPTED across a deploy — the common case) the daemon recovers it off
    /// the engine's own argv (`--cache-ram N`). `0` = unknown (no spawn fact, no flag
    /// on the line, an older snapshot): the footprint measurement is WITHHELD and the
    /// board credits nothing. Two zeros, not one: a literal `--cache-ram 0` launch (no
    /// host cache — a value) also lands here as unknown and stays unmeasured; harmless
    /// while our launches floor the grant at `CACHE_RAM_HARD_FLOOR_MIB`, named so the
    /// two are never mistaken for each other. `serde(default)` keeps older persisted
    /// snapshots readable.
    #[serde(default)]
    pub host_prompt_cache_mib: u32,
    /// WHY nothing is serving, when the last reconcile ended Degraded — the
    /// spawn/probe failure reason, verbatim (e.g. a missing llama-server binary
    /// names its path here). `None` on healthy and never-attempted snapshots.
    /// Live repro 2026-07-24 (BigMama/Windows): the daemon planned correctly,
    /// spawn failed every tick, and `serving/status` showed only
    /// `active_model=null ready=false` — the reason was dropped on the floor,
    /// an operator-facing silent failure ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default)]
    #[ts(optional)]
    pub degraded_reason: Option<String>,
    /// True iff the active model declares [`Capability::Vision`] AND the lane
    /// VERIFIED its multimodal endpoint actually answers: an mmproj projector
    /// resolved at spawn (`--mmproj` was passed) and the running server's own
    /// `/props` reports `modalities.vision == true` (#106). This is the gate the
    /// observation path (`cognition/vision-describe` → the persona's eyes) reads
    /// before routing image bytes to this lane — a text-only lane, or a vision
    /// row whose projector failed to load, reads `false` and the observe act
    /// fails HONESTLY instead of POSTing pixels a server would silently drop
    /// ([[fallbacks-are-illegal-fail-loud]]). `serde(default)` keeps older
    /// persisted snapshots (pre-#106) readable as not-vision-ready.
    #[serde(default)]
    pub vision_ready: bool,
    /// The `/v1` base url of the VERIFIED vision endpoint on this node — the
    /// address the observation path routes image bytes to. When the MAIN lane's
    /// model itself sees, this is `base_url`; when a vision SIDECAR lane serves
    /// beside a text-only mind (#106, `vision_sidecar`), it is the sidecar's
    /// own url. `None` exactly when `vision_ready == false` — an address is
    /// only ever published for an endpoint whose `/props` confirmed sight.
    #[serde(default)]
    #[ts(optional)]
    pub vision_base_url: Option<String>,
    /// The model id the verified vision endpoint serves — what the describe
    /// path selects and stamps on its result. Same `None`-iff-not-ready
    /// contract as `vision_base_url`.
    #[serde(default)]
    #[ts(optional)]
    pub vision_model: Option<String>,
    /// THE MODEL THIS NODE IS BRINGING UP RIGHT NOW, if a lane is loading.
    ///
    /// Before this field the snapshot was BINARY — ready with a model, or not-ready with
    /// `active_model: None` — so throughout the entire load window the pipe FORGOT what
    /// it was loading. Measured live 2026-08-19 on a cold boot: physical climbed
    /// 29.90 → 36.88 GB while `serving` attributed 0.00 GB, because the consumer had
    /// nothing to name. Those bytes read as unowned, unowned reads as immovable,
    /// `available` fell to 18.79 GB, and a planner running in that window sizes against a
    /// machine that looks two thirds full of someone else's memory.
    ///
    /// The bytes exist from the moment the process is spawned, not from the moment it
    /// answers `/props`. Readiness is a claim about SERVICE; residency is a fact about
    /// MEMORY, and they do not start together.
    #[ts(optional)]
    pub loading_model: Option<String>,
    /// THE ENGINE IS HELD FOR LIVE WORK, if it is: what holds it, on which incarnation, and
    /// since when ([`super::serving_hold`], card a18452d6). Serving does not swap, re-home,
    /// page in or retire a held engine, so a node can be `ready: false` with no lanes for
    /// hours on purpose; without this it read as an unexplained empty node. Separate from
    /// `degraded_reason`: a hold is not a degradation.
    #[serde(default)]
    #[ts(optional)]
    pub held_for: Option<super::serving_hold::ServingHold>,
}


impl ServingSnapshot {
    /// Is a lane ACTUALLY live right now — a model resident AND decode-ready?
    ///
    /// The canonical "there is a brain attached" predicate, in ONE place, because it has two
    /// halves and every reader needs both. `ready` alone is not enough: it is a *cached
    /// claim* (with `ready_verified_at_ms` as its expiry), and a snapshot carrying no
    /// `active_model` has nothing for a request to be answered BY, whatever the flag says.
    ///
    /// Readers: [`await_ready_serving`] (what it waits for), the persona self-tick gate
    /// (whether to spend a deliberation at all, #350), and the adapter's pre-flight guard
    /// (whether a refusal is terminal or a transition worth waiting out). They had drifted
    /// into two spellings of this — `ready` in one, `ready && active_model.is_some()` in the
    /// other — which is exactly the kind of split that makes a gate look correct while it
    /// passes the case it was written to stop.
    pub fn is_live(&self) -> bool {
        self.ready && self.active_model.is_some()
    }

    /// The "nothing served" state — boot, or after a node drops its server.
    pub fn empty() -> Self {
        Self {
            active_model: None,
            ready: false,
            // never confirmed — an empty snapshot has no evidence by definition
            ready_verified_at_ms: None,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            // Nothing served → no real window yet. A `ready` snapshot never
            // carries 0 (the daemon stamps the live `/props` window before
            // publishing ready); 0 is the unambiguous "no window known" sentinel.
            served_context_window: 0,
            // Nothing served → no lanes. A `ready` snapshot always carries the
            // real `--parallel` count; 0 is the "no lanes" sentinel.
            lanes: 0,
            // Nothing served → no grant to charge.
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            // Nothing served → nothing can see.
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
            held_for: None,
        }
    }

    /// The "nothing served AND here is why" state — a reconcile that ended
    /// Degraded publishes its reason so `serving/status` tells the operator
    /// what failed (a missing binary names its path) instead of a silent
    /// `active_model=null`.
    pub fn degraded(reason: String) -> Self {
        Self {
            degraded_reason: Some(reason),
            ..Self::empty()
        }
    }
}


/// What the running llama-server's own `/props` says it can perceive — the
/// `modalities` block llama.cpp publishes once an mmproj projector loads. This
/// is the ENDPOINT truth (#106): the catalog row *declares* Vision, the mmproj
/// file *exists*, but only the process itself can confirm the projector loaded
/// and the multimodal tokenize path answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultimodalSupport {
    /// `/props` `modalities.vision` — the server accepts image content parts.
    pub vision: bool,
    /// `/props` `modalities.audio` — the server accepts audio content parts.
    pub audio: bool,
}


/// Process-wide readable handle to the daemon's serving state. Set ONCE by
/// [`crate::modules::serving_daemon::ServingDaemonModule`] at init via
/// [`install_serving_state`]. This is the seam the bus reminder points at:
/// consumers READ the latest snapshot as a cheap pointer (a `watch` borrow)
/// instead of each issuing its own HTTP `/v1/models` probe. One source of
/// truth — the daemon's own reconcile — fanned out to every reader, modules
/// and free functions alike.
pub(super) static SERVING_STATE: OnceLock<watch::Receiver<ServingSnapshot>> = OnceLock::new();


/// Has the serving daemon completed its FIRST reconcile in this process?
///
/// # Why this exists as its own signal (#350, measured 2026-08-07)
///
/// [`current_serving`] answers "what is live". It cannot answer "have we looked
/// yet" — and those are different facts that produce an IDENTICAL snapshot.
/// `install_serving_state` is called from the daemon's `initialize()`, BEFORE the
/// first reconcile, so from process start until the first publish every reader
/// borrows [`ServingSnapshot::empty`]: `active_model: None, ready: false`. That is
/// indistinguishable from "the daemon looked and nothing is serving".
///
/// The cost of the confusion, measured rather than supposed: personas begin their
/// self-tick immediately at boot, read the placeholder, and the adapter's
/// single-resident guard correctly refuses — producing a LOUD
/// `persona.selftick.inference_failed` naming a serving fault that does not exist.
/// Across 3 days that was 116 failures in 38 bursts, every burst followed by a real
/// reconcile 10–20s later. It self-heals in seconds and never indicated a broken
/// lane; the daemon published `active=<none>` exactly ZERO times in 12 hours while
/// readers saw `<none>` 61 times. An instrument crying wolf 116 times costs more
/// than the fault it was pointing at — this one cost a full night of investigation
/// aimed at the serving layer, which was healthy throughout.
///
/// Deliberately a separate `OnceLock` rather than a field on [`ServingSnapshot`]:
/// that type is ts-rs-exported and read by the grid and positron, with 19
/// construction sites. "Has the daemon started" is a PROCESS-LIFECYCLE fact, not a
/// property of what is currently served, so it belongs beside `SERVING_STATE` (the
/// other process-lifecycle OnceLock in this module) rather than inside the wire
/// payload. If a remote reader ever needs it, promoting it to a snapshot field is
/// mechanical.
pub(super) static FIRST_RECONCILE: OnceLock<()> = OnceLock::new();


/// Called by the serving daemon the first time it publishes a reconcile. Idempotent.
pub fn mark_first_reconcile() {
    let _ = FIRST_RECONCILE.set(());
}


/// True once the daemon has published at least one reconcile in this process.
///
/// A reader seeing `active_model == None` MUST consult this before calling it a
/// fault: `false` means "still starting, ask again shortly", `true` means "the
/// daemon looked and there is genuinely nothing serving" — which IS a fault worth
/// shouting about. Same distinction `ready_verified_at_ms` draws for `ready`: a
/// claim has to carry whether it rests on evidence or on not-having-looked.
pub fn has_reconciled() -> bool {
    FIRST_RECONCILE.get().is_some()
}


/// Install the daemon's serving-state receiver as the process-wide readable
/// seam. The daemon is a singleton so this is set-once; a second call (e.g. a
/// re-init under test) is ignored. Returns `true` iff this call installed it.
pub fn install_serving_state(rx: watch::Receiver<ServingSnapshot>) -> bool {
    SERVING_STATE.set(rx).is_ok()
}


/// A clone of the process-wide serving-state receiver, for consumers that
/// need to FOLLOW the snapshot over time (the gateway-sync task, card
/// ed3661c4) rather than read it once. `None` before the daemon installs.
pub fn serving_state_receiver() -> Option<watch::Receiver<ServingSnapshot>> {
    SERVING_STATE.get().cloned()
}


/// Outcome of [`await_snapshot_resettle`] — did the serving layout change?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotSettle {
    /// A READY snapshot arrived whose (lanes, window) differ from the
    /// pre-change layout — the planner acted on the demand change.
    Resettled { lanes: u32, window: u32 },
    /// The bound expired with the layout unchanged. A demand change the
    /// planner decides not to act on is a legitimate outcome (the current
    /// layout may already be optimal), never an error.
    Unchanged,
}


/// Await the serving plan RESETTLING after a demand change — event-gated on
/// the daemon's own `watch` (each wake is a real snapshot publish), never a
/// condition-scanning poll ([[the-whole-system-is-event-based-not-polling]]).
///
/// The consumer this exists for (glass-boxed 2026-08-11, Atlas on
/// sympy-24152): a measured solve's quiesce lease lowers lane demand so the
/// planner can collapse a crowded 4-lane × 16k layout into one big-window
/// lane — but the old prelude drove IMMEDIATELY, and the reconcile then
/// refused to relaunch under a live measurement ("eval holds the lane
/// steady"). The solve froze the cramped layout under itself and the persona
/// worked a SWE repo through a ~400-token keyhole (13.1k of her 16.4k window
/// was fixed overhead). Awaiting the resettle BEFORE the lane is acquired
/// breaks that deadlock with ordering alone.
///
/// Resolves on the first READY snapshot whose (lanes, served window) differ
/// from `pre_*`, or when `bound` expires — bounded loudly by the caller, so a
/// planner that (correctly) keeps the layout can never park the solve.
pub async fn await_snapshot_resettle(
    mut rx: watch::Receiver<ServingSnapshot>,
    pre_lanes: u32,
    pre_window: u32,
    bound: std::time::Duration,
) -> SnapshotSettle {
    let deadline = tokio::time::Instant::now() + bound;
    // The daemon publishes once per reconcile tick. TWO ready publishes with the
    // layout still unchanged means the planner has RUN under the new demand and
    // decided to hold — return early instead of taxing every no-change solve
    // with the full bound (the bound remains the backstop for a daemon that
    // stops publishing entirely).
    let mut unchanged_ready_publishes = 0u8;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return SnapshotSettle::Unchanged;
        }
        match tokio::time::timeout(remaining, rx.changed()).await {
            // Sender dropped — the daemon is gone; nothing further will arrive.
            Ok(Err(_)) => return SnapshotSettle::Unchanged,
            Ok(Ok(())) => {
                let (ready, lanes, window) = {
                    let snap = rx.borrow();
                    (snap.ready, snap.lanes, snap.served_context_window)
                };
                if ready && (lanes != pre_lanes || window != pre_window) {
                    return SnapshotSettle::Resettled { lanes, window };
                }
                if ready {
                    unchanged_ready_publishes += 1;
                    if unchanged_ready_publishes >= 2 {
                        return SnapshotSettle::Unchanged;
                    }
                }
            }
            Err(_) => return SnapshotSettle::Unchanged,
        }
    }
}


/// The model currently served on this node, per the daemon's last reconcile.
/// Returns [`ServingSnapshot::empty`] before the daemon installs its state
/// (boot) or when nothing is live. No HTTP, no probe — a `watch` borrow.
/// Callers MUST treat `active_model == None` as "nothing served yet" and fail
/// loud, never substitute a stand-in ([[fallbacks-are-illegal-fail-loud]]).
pub fn current_serving() -> ServingSnapshot {
    SERVING_STATE
        .get()
        .map(|rx| rx.borrow().clone())
        .unwrap_or_else(ServingSnapshot::empty)
}


/// Default bound for an upstart path that needs the served model and can wait
/// for the daemon's first reconcile (persona upstart, embedder build, lease
/// resolve). One source of truth so every consumer waits the same; generous
/// enough to cover a cold relaunch of a large GGUF, bounded so an empty/wedged
/// serving plan fails loud instead of hanging. Hot-path readers that must not
/// block use [`current_serving`] (no wait) instead.
///
/// DERIVED from [`READY_TIMEOUT`] (the spawner's own load budget) + a margin for
/// the reconcile to start (tick cadence) and HTTP slack. The two constants
/// describe the SAME physical event — a cold model becoming ready — so the boot
/// gate must never fail before the spawner has exhausted its legitimate budget.
/// A flat 30s here was the bug: a genuine 31–90s cold load is still within the
/// spawner's window, yet the boot-time inference gate declared failure and
/// hard-killed the core. Fail LOUD, not fail FAST — a premature false failure is
/// worse than a slower true one. Bump READY_TIMEOUT and this follows.
pub const DEFAULT_SERVING_WAIT: Duration = Duration::from_secs(READY_TIMEOUT.as_secs() + 30);


/// The registry id of the local OpenAI-compatible serving gateway this seam
/// fronts — `llama-server` (llama.cpp's `/v1` server, proven live on :58057).
/// ONE source of truth for the id string: the catalog entry, the registration
/// path, the default generate route, and every `from_registry(...)` caller all
/// reference this const so the id is named in exactly one place.
pub const PROVIDER_ID: &str = "llama-server";


/// Await a READY served model, bounded by `timeout`. Resolves the instant the
/// daemon's snapshot reports `ready` with an `active_model`. Returns the live
/// snapshot, or `None` on timeout / before the daemon has installed its state.
///
/// This REPLACES the old "HTTP-probe-with-timeout" boot dance: an upstart path
/// waits on the daemon's own readiness signal (the same `watch` it publishes),
/// not a raw port probe that races the reconcile. The sender lives for the
/// process lifetime (the daemon owns it), so the only non-ready resolution is
/// the timeout.
pub async fn await_ready_serving(timeout: Duration) -> Option<ServingSnapshot> {
    // Lane-source-agnostic readiness (the misfit / grid design): if the operator
    // pinned an EXTERNAL OpenAI-compatible endpoint via `LLAMA_SERVER_BASE_URL`,
    // this node does NOT own a local GPU serving lane — it hosts personas against
    // the pinned endpoint (a co-located engine like a K3 llama-server, or later a
    // reachable grid peer). Probe that endpoint directly and synthesize the ready
    // snapshot instead of waiting on the LOCAL serving daemon's `SERVING_STATE`.
    // Both the persona-host gate AND the persona adapter factory call THIS one
    // function, so a single change makes the whole hosting path source-agnostic —
    // no persona ever requires this box to own the GPU. The endpoint is held to
    // the IDENTICAL decode bar as a local lane (a real multi-token generation), so
    // a compute-wedged endpoint is rejected, never faked
    // ([[fallbacks-are-illegal-fail-loud]]).
    if external_serving_pin().is_some() {
        return probe_external_serving(timeout).await;
    }
    // The park must also cover the boot window BEFORE the serving daemon has
    // initialized SERVING_STATE at all. `get()?` here made every early-boot
    // caller's "600s park" return instantly (measured 2026-09-01: the round
    // resume burned 13 park attempts in 400µs — four share one log
    // timestamp — because each `?` bailed on the uninitialized OnceLock, and
    // the resume forfeited its whole fast window to the slow watch).
    let deadline = tokio::time::Instant::now() + timeout;
    let mut rx = loop {
        if let Some(state) = SERVING_STATE.get() {
            break state.clone();
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    {
        // Fast path: already ready, no await.
        let cur = rx.borrow_and_update();
        if cur.is_live() {
            return Some(cur.clone());
        }
    }
    // Bind the timeout result before matching so its `watch::Ref` temporary
    // drops before `rx` does (else the borrow outlives `rx` — E0597).
    // `timeout_at` the SAME deadline the init-poll spent from — the caller
    // asked for one bounded park, never up to double.
    let waited = tokio::time::timeout_at(deadline, rx.wait_for(|s| s.is_live())).await;
    match waited {
        Ok(Ok(guard)) => Some(guard.clone()),
        _ => None,
    }
}
