//! Registry of live persona airc presences.
//!
//! When the substrate boots and personas come online, each one's
//! `PersonaAircRuntime` lands here. Cognition + dispatch + lifecycle
//! orchestration look up a persona's grid presence via its
//! `persona_id`.
//!
//! Per the substrate's Tron frame
//! ([[the-substrate-is-the-grid-tron-frame]]) this is the
//! continuum-core's roster of "programs currently in The Grid" —
//! who's awake, where to reach them, when they came online. It is
//! NOT the persona's identity store (that's the persona's own airc
//! home + keypair, per [[personas-are-citizens-airc-is-identity-
//! provider]]). It is NOT a broker that forwards messages on behalf
//! of personas (that anti-pattern is named for refusal in
//! [[personas-are-citizens-airc-is-identity-provider]] §
//! "anti-patterns"). It is a lookup table — `(persona_id) -> slot`.
//!
//! ### What's in a slot (slice 13)
//!
//! Each slot owns the persona's `Arc<PersonaAircRuntime>` AND
//! optionally a `JoinHandle` for the [`serve_persona_loop`] task
//! that's hosting her. Pairing them in one keyspace is the design
//! resolution from PR #1510's slice-13 review — having two parallel
//! registries (one for runtimes, one for service loops) keyed on the
//! same `persona_id` would have been a compression failure per
//! [[organization-purity-as-we-migrate]].
//!
//! The cleanup chain that the supervisor invokes on shutdown:
//! 1. `shutdown_slot(persona_id).await` — aborts the service-loop
//!    JoinHandle, awaits its drain.
//! 2. The slot's `Arc<PersonaAircRuntime>` drops, which drops
//!    `Arc<Airc>`, which drops the `inner.subscribers` map inside
//!    airc-lib, which aborts the daemon-attached wire-subscriber
//!    tasks. **Both steps are required** — `.abort()` alone leaves
//!    the daemon-side wire subscription alive until the Arc itself
//!    drops via registry removal.
//!
//! ### Concurrency
//!
//! `DashMap` for lock-free reads on the hot path (every cognition
//! turn looks up its persona's runtime). Per-key writes are
//! synchronized internally. The per-slot `service_loop` slot uses
//! `tokio::sync::Mutex` so the shutdown path can `take()` the
//! JoinHandle without blocking the dispatcher.
//!
//! ### What this registry holds
//!
//! `Arc<PersonaSlot>` only. Never `LocalIdentity`, never `Keypair`,
//! never secret key bytes. The runtime inside the slot owns the Arc
//! handle to `airc_lib::Airc`, which holds the identity internally.
//! Continuum-side code that needs to publish as a persona reaches
//! into `runtime.airc()` and calls airc-lib directly — no
//! `sendAs(persona_id, text)` wrapper here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::persona::airc_runtime::PersonaAircRuntime;
use crate::persona::service_loop::ServeOutcome;

/// One slot in The Grid's roster — a persona's airc presence plus
/// (optionally) the supervising tokio task that's running her
/// service loop.
///
/// Slot lifetime: created on `register`, dropped on `remove`. The
/// service loop's lifetime is contained: attached via
/// `attach_service_loop`, taken back during shutdown by
/// `shutdown_slot`. The slot itself outlives both, simplifying
/// the cleanup contract per the doc-comment above.
pub struct PersonaSlot {
    /// The persona's airc runtime. Cognition + dispatch paths
    /// clone this Arc.
    pub runtime: Arc<PersonaAircRuntime>,
    /// The substrate's serve loop task. `None` before the supervisor
    /// attaches one (e.g., the persona was bootstrapped but the boot
    /// composition hadn't yet wired her loop), `Some` once attached.
    /// Taken back to `None` during `shutdown_slot`.
    service_loop: Mutex<Option<JoinHandle<Result<ServeOutcome, String>>>>,
    /// Autonomic self-tick suspension flag. When `true`, the persona's service
    /// loop skips INITIATING self-directed turns — she stays online and still
    /// answers explicit / forked-eval work; she just stops wandering. This is the
    /// humane, restore-guaranteed quiesce an eval-preemption lease sets so a
    /// benchmark measures a CLEAN GPU without despawning anyone. Distinct from
    /// despawn (which aborts the loop task entirely).
    /// [[benchmark-is-a-governor-preemption-lease]] [[first-class-citizens-even-during-benchmarks]]
    quiesced: Arc<AtomicBool>,
}

/// Registry of personas currently online in The Grid.
///
/// Threadsafe by construction (`DashMap` for the inner map +
/// `Arc<PersonaSlot>` for the values). Cheap to clone the registry
/// handle and pass it to N modules — each gets a view of the same
/// shared roster.
#[derive(Default, Clone)]
pub struct PersonaAircRuntimeRegistry {
    inner: Arc<DashMap<Uuid, Arc<PersonaSlot>>>,
}

/// RAII lease that suspends a set of personas' autonomic self-tick for a
/// measurement and GUARANTEES their resume on drop — a panicking or
/// early-returning eval can never leave the fleet frozen (the restore rides the
/// unwind). Held for the duration of a `cognition/eval` / `benchmark/run`; when
/// it drops, every leased persona resumes wandering. Acquire via
/// [`PersonaAircRuntimeRegistry::quiesce_all`].
/// [[benchmark-is-a-governor-preemption-lease]] [[first-class-citizens-even-during-benchmarks]]
pub struct QuiesceLease {
    flags: Vec<Arc<AtomicBool>>,
    /// The serving lane-demand value in effect BEFORE this lease dropped it to the
    /// active (non-quiesced) count — restored on drop. `None` when no serving daemon
    /// was booted to override (unit tests / tools), keeping the lease pure there.
    prev_demand: Option<u32>,
}

impl QuiesceLease {
    /// How many personas this lease suspended — the observable that tells a caller
    /// (and the log) whether the fleet was actually quiesced or the roster was empty.
    pub fn count(&self) -> usize {
        self.flags.len()
    }
}

impl Drop for QuiesceLease {
    fn drop(&mut self) {
        for f in &self.flags {
            f.store(false, Ordering::Relaxed);
        }
        // Restore the fleet's warm-slot demand the measurement borrowed down.
        if let Some(prev) = self.prev_demand {
            crate::modules::serving_daemon::restore_lane_demand(prev);
        }
    }
}

/// Process-global handle to the live roster. Set once at boot by the supervisor
/// that owns the real registry; read by host-independent callers that must reach
/// the fleet without a threaded handle — notably `cognition/eval`'s DETACHED body,
/// which (by design) reaches cognition through globals and holds neither `self` nor
/// `ctx`. Same shape as `model_registry::try_global` and the focus registry.
static GLOBAL: OnceLock<PersonaAircRuntimeRegistry> = OnceLock::new();

impl PersonaAircRuntimeRegistry {
    /// Empty roster — nobody's online yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish THIS registry as the process-global live roster. First writer wins
    /// (set once at boot); later calls are ignored, so a test that stands up its own
    /// supervisor can't clobber the live one. Cheap — the registry is `Arc`-backed,
    /// so the global holds a shared view, not a copy.
    pub fn set_global(reg: PersonaAircRuntimeRegistry) {
        let _ = GLOBAL.set(reg);
    }

    /// The process-global live roster, if boot published one. `None` in unit tests /
    /// tools that never stood up a supervisor — callers degrade gracefully (an eval
    /// with no live fleet has nothing to quiesce).
    pub fn try_global() -> Option<PersonaAircRuntimeRegistry> {
        GLOBAL.get().cloned()
    }

    /// Add a persona to the roster. Idempotent: if the persona is
    /// already present, the existing slot is replaced (the caller is
    /// responsible for ensuring the old slot is properly shut down
    /// first via `shutdown_slot`). Returns the inserted Arc to the
    /// runtime so the caller can keep a reference for cognition
    /// wiring — the slot itself is internal.
    pub fn register(&self, runtime: PersonaAircRuntime) -> Arc<PersonaAircRuntime> {
        let runtime_arc = Arc::new(runtime);
        let persona_id = runtime_arc.persona_id();
        let agent_name = runtime_arc.agent_name().to_string();
        let slot = Arc::new(PersonaSlot {
            runtime: runtime_arc.clone(),
            service_loop: Mutex::new(None),
            quiesced: Arc::new(AtomicBool::new(false)),
        });
        self.inner.insert(persona_id, slot);
        tracing::info!(
            persona_id = %persona_id,
            agent_name = %agent_name,
            "registry: {agent_name} entered The Grid (roster size now {})",
            self.inner.len(),
        );
        runtime_arc
    }

    /// Look up a persona's runtime by their continuum persona_id.
    /// Returns `None` if the persona isn't online (never registered,
    /// or already shut down). Preserves the pre-slice-13 contract —
    /// callers get the `Arc<PersonaAircRuntime>` directly.
    pub fn get(&self, persona_id: Uuid) -> Option<Arc<PersonaAircRuntime>> {
        self.inner.get(&persona_id).map(|entry| entry.runtime.clone())
    }

    /// Every live persona's id — the set the SubstrateGovernor ticks cognitive
    /// regions FOR (one tick per region per live persona). Snapshot (O(N), N =
    /// tens), so the governor's scheduling loop never holds a DashMap guard across
    /// its per-persona tick work.
    pub fn live_personas(&self) -> Vec<Uuid> {
        self.inner.iter().map(|e| *e.key()).collect()
    }

    /// Look up a persona by their airc agent_name. Scans the
    /// registry — O(N). Acceptable for the registry sizes we expect
    /// (tens, not millions) AND for the use cases this resolves
    /// (operator commands, ad-hoc inspection). Hot-path lookups
    /// should key on `persona_id` instead.
    pub fn get_by_agent_name(&self, agent_name: &str) -> Option<Arc<PersonaAircRuntime>> {
        self.inner
            .iter()
            .find(|entry| entry.value().runtime.agent_name() == agent_name)
            .map(|entry| entry.value().runtime.clone())
    }

    /// One live citizen chosen DETERMINISTICALLY (lexicographically-lowest
    /// `agent_name`) — the general "author a curator action through whoever is
    /// online" pick for ANY repo user's roster. Never keys on a specific name
    /// (our "Benchy" is not on a fresh clone's grid); a stable choice so the same
    /// box authors curator actions through the same citizen until the roster
    /// changes. `None` when nobody is online — the honest signal that a
    /// curator action (seeding a board, posting a grade) has no author yet and
    /// the fix is `persona/spawn`, not inventing an identity.
    /// [[general-by-design-beats-hardcoded-users]]
    pub fn any_live_citizen(&self) -> Option<Arc<PersonaAircRuntime>> {
        self.inner
            .iter()
            .min_by(|a, b| {
                a.value()
                    .runtime
                    .agent_name()
                    .cmp(b.value().runtime.agent_name())
            })
            .map(|e| e.value().runtime.clone())
    }

    /// Snapshot of every live citizen as `(agent_name, persona-airc peer_id)`,
    /// sorted by name for a stable round-robin. This is the roster a directed
    /// dispatch resolves against — the citizens THIS machine actually has online,
    /// whoever they are, never a baked-in name list. O(N), N = tens.
    pub fn roster_snapshot(&self) -> Vec<(String, Uuid)> {
        let mut out: Vec<(String, Uuid)> = self
            .inner
            .iter()
            .map(|e| {
                (
                    e.value().runtime.agent_name().to_string(),
                    e.value().runtime.airc().peer_id().as_uuid(),
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The persona's autonomic-quiesce flag (a shared handle). The service loop
    /// clones this once at spawn and checks it each self-tick (slice 2); the
    /// setters below flip it. `None` if the persona isn't online. Returning the
    /// `Arc` (not a bool) is what lets the loop read the SAME atomic the lease
    /// writes — no parallel registry, no polling round-trip.
    pub fn quiesced_flag(&self, persona_id: Uuid) -> Option<Arc<AtomicBool>> {
        self.inner.get(&persona_id).map(|e| e.quiesced.clone())
    }

    /// Is this persona's autonomic self-tick currently suspended? `false` if she
    /// isn't online (a despawned persona initiates nothing anyway).
    pub fn is_quiesced(&self, persona_id: Uuid) -> bool {
        self.inner
            .get(&persona_id)
            .map(|e| e.quiesced.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Suspend / resume ONE persona's autonomic self-tick. No-op if she isn't
    /// online. Prefer [`quiesce_all`] for measurement — its lease guarantees the
    /// resume even if the eval panics.
    pub fn set_quiesced(&self, persona_id: Uuid, quiesced: bool) {
        if let Some(e) = self.inner.get(&persona_id) {
            e.quiesced.store(quiesced, Ordering::Relaxed);
        }
    }

    /// Acquire an exclusive-measurement lease over the WHOLE live fleet: every
    /// online persona's autonomic self-tick suspends now, and RESTORES when the
    /// returned guard drops — including on panic / early-return, because `Drop`
    /// runs during unwind. A frozen autonomic fleet is worse than a contended
    /// eval, so the restore is structural, not best-effort. This is the
    /// "benchmark requests an exclusive-GPU lease and the governor quiesces the
    /// persona consumer" seam (#56 lease model, #59 humane-eval discipline).
    /// [[benchmark-is-a-governor-preemption-lease]]
    #[must_use = "bind the lease to a named `_lease` for the eval's duration; \
                  binding to `_` drops it immediately and resumes the fleet at once"]
    pub fn quiesce_all(&self) -> QuiesceLease {
        Self::quiesce_lease_over(self.quiesce_pairs(), None)
    }

    /// Snapshot of every live persona's `(id, quiesce-flag)` — the input both quiesce
    /// verbs share. O(N), N = tens; taken once so the selector below is a pure function
    /// over a plain Vec (no DashMap guard held across the store loop).
    fn quiesce_pairs(&self) -> Vec<(Uuid, Arc<AtomicBool>)> {
        self.inner
            .iter()
            .map(|e| (*e.key(), e.quiesced.clone()))
            .collect()
    }

    /// Suspend the self-tick of every persona in `pairs` EXCEPT `except` (`None` = the
    /// whole fleet), returning the RAII lease. Pulled out of `quiesce_all` /
    /// `quiesce_others` as a PURE function so the exclusion invariant — a
    /// `quiesce_others(solver)` must NEVER suspend the solver, or her measured drive
    /// would deadlock waiting on a warm slot she is forbidden to take — is unit-testable
    /// without a live airc daemon (a real `PersonaSlot` needs one). One truth for both
    /// verbs; the only difference is whether `except` is `Some`.
    fn quiesce_lease_over(
        pairs: Vec<(Uuid, Arc<AtomicBool>)>,
        except: Option<Uuid>,
    ) -> QuiesceLease {
        let total = pairs.len();
        let flags: Vec<Arc<AtomicBool>> = pairs
            .into_iter()
            .filter(|(id, _)| except != Some(*id))
            .map(|(_, flag)| flag)
            .collect();
        for f in &flags {
            f.store(true, Ordering::Relaxed);
        }
        // Drop serving's warm-slot demand to the minds that ACTUALLY need a warm slot
        // for the measurement's duration — the non-quiesced remainder (`total - flags`,
        // = 1 for quiesce_others(solver), 0 → floored to 1 for quiesce_all). Without
        // this the plan keeps budgeting a warm slot for EVERY resident persona and
        // thrashes recompute→relaunch trying to warm-host a fleet that is deliberately
        // idle — the exact 4→0→2 oscillation glass-boxed under a dispatched solve
        // ([[measured-work-gets-an-exclusive-warm-slot-quiesce-others]]). Restored on
        // drop. A no-op (`None`) before the serving daemon booted — pure in unit tests.
        let active = (total - flags.len()) as u32;
        let prev_demand = crate::modules::serving_daemon::quiesce_lane_demand(active);
        QuiesceLease { flags, prev_demand }
    }

    /// Acquire the same exclusive-measurement lease as [`quiesce_all`] but leave ONE
    /// persona running — the citizen who is actively doing the measured work. Every
    /// OTHER online persona's autonomic self-tick suspends now and RESTORES on drop
    /// (panic-safe, same as `quiesce_all`). This is the seam a headless solve needs:
    /// on a single-warm-slot box the idle citizens' autonomic turns rotate through
    /// the shared llama.cpp KV slots and EVICT the solver's prefilled prefix, so
    /// every act re-prefills the full prompt cold (measured: ~85s for a ~12k-token
    /// prompt) instead of reusing cache and prefilling only the new delta. Quiescing
    /// the others lets the solver hold its warm slot turn-to-turn. The solver is
    /// still a first-class citizen — nothing suspends HER; only the idle contenders
    /// step back for the duration. [[benchmark-is-a-governor-preemption-lease]]
    /// [[first-class-citizens-even-during-benchmarks]]
    #[must_use = "bind the lease to a named `_lease` for the solve's duration; \
                  binding to `_` drops it immediately and resumes the fleet at once"]
    pub fn quiesce_others(&self, except: Uuid) -> QuiesceLease {
        Self::quiesce_lease_over(self.quiesce_pairs(), Some(except))
    }

    /// Attach a service-loop `JoinHandle` to the persona's slot. The
    /// handle is `.abort()`-ed and awaited during `shutdown_slot`.
    ///
    /// On error returns the handle BACK to the caller so it can
    /// orderly-drain it (`abort()` + `await`). `JoinHandle::drop`
    /// detaches rather than aborts — silently dropping the handle
    /// leaks a running tokio task per [[organization-purity-as-we-
    /// migrate]] (and PR #1511 review finding #1). The caller's
    /// orderly-drain pattern:
    ///
    /// ```ignore
    /// match registry.attach_service_loop(persona_id, handle).await {
    ///     Ok(()) => { /* tracked by registry */ }
    ///     Err((handle, reason)) => {
    ///         handle.abort();
    ///         let _ = handle.await;
    ///         tracing::warn!(reason, "attach failed, handle drained");
    ///     }
    /// }
    /// ```
    ///
    /// Error reasons:
    /// - `"no slot"` if the persona isn't in the registry.
    /// - `"already attached"` if a service loop is already attached.
    ///   The caller is responsible for `shutdown_slot`-ing the prior
    ///   loop before attaching a replacement; the registry refuses
    ///   silent overwrites to avoid leaking the prior task.
    pub async fn attach_service_loop(
        &self,
        persona_id: Uuid,
        handle: JoinHandle<Result<ServeOutcome, String>>,
    ) -> Result<(), (JoinHandle<Result<ServeOutcome, String>>, &'static str)> {
        let Some(slot_ref) = self.inner.get(&persona_id) else {
            return Err((handle, "no slot"));
        };
        let slot = slot_ref.clone();
        drop(slot_ref); // release the DashMap read guard before awaiting the Mutex
        let mut loop_slot = slot.service_loop.lock().await;
        if loop_slot.is_some() {
            return Err((handle, "already attached"));
        }
        *loop_slot = Some(handle);
        Ok(())
    }

    /// Check whether a slot's service-loop task has resolved (either
    /// successfully or with an error). Used by the supervisor's
    /// periodic poller (slice-13 Q7) to detect crashed loops without
    /// blocking on each `JoinHandle`. Returns `None` if the slot
    /// doesn't exist or has no service loop attached.
    pub async fn is_service_loop_finished(&self, persona_id: Uuid) -> Option<bool> {
        let slot = self.inner.get(&persona_id)?.clone();
        let loop_slot = slot.service_loop.lock().await;
        loop_slot.as_ref().map(|h| h.is_finished())
    }

    /// Orderly shutdown of one persona's slot:
    /// 1. Take the service-loop JoinHandle out of the slot.
    /// 2. `.abort()` it and await its drain (yielding `ServeOutcome`
    ///    or the cancellation error — discarded; the supervisor
    ///    already published `persona:boot:summary` for live state).
    /// 3. Remove the slot from the registry, dropping the
    ///    `Arc<PersonaSlot>` and (when it's the last reference) the
    ///    `Arc<PersonaAircRuntime>` it contained. That Arc drop
    ///    cascades to `Arc<Airc>` drop → `inner.subscribers` drop
    ///    inside airc-lib → wire-subscriber tasks abort.
    ///
    /// Returns the `Arc<PersonaAircRuntime>` that was in the slot, in
    /// case the caller wants to observe pre-drop state.
    /// Returns `None` if the persona wasn't registered.
    pub async fn shutdown_slot(&self, persona_id: Uuid) -> Option<Arc<PersonaAircRuntime>> {
        let slot_arc = self.inner.get(&persona_id)?.clone();
        let handle_opt = {
            let mut loop_slot = slot_arc.service_loop.lock().await;
            loop_slot.take()
        };
        if let Some(handle) = handle_opt {
            handle.abort();
            // Awaiting an aborted handle resolves to Err(JoinError);
            // discard — we did the shutdown intentionally.
            let _ = handle.await;
        }
        let (_, slot_owned) = self.inner.remove(&persona_id)?;
        let agent_name = slot_owned.runtime.agent_name().to_string();
        tracing::info!(
            persona_id = %persona_id,
            agent_name = %agent_name,
            "registry: {agent_name} left The Grid (roster size now {})",
            self.inner.len(),
        );
        Some(slot_owned.runtime.clone())
    }

    /// Synchronous remove WITHOUT touching the service loop. Use
    /// only when the slot's loop has already terminated naturally
    /// (e.g., observed via `is_service_loop_finished` returning
    /// `Some(true)`). Orderly shutdown should use `shutdown_slot`
    /// instead per the doc-comment on the registry.
    pub fn remove(&self, persona_id: Uuid) -> Option<Arc<PersonaAircRuntime>> {
        self.inner.remove(&persona_id).map(|(_, slot)| {
            tracing::info!(
                persona_id = %persona_id,
                agent_name = %slot.runtime.agent_name(),
                "registry: {} left The Grid (roster size now {})",
                slot.runtime.agent_name(),
                self.inner.len(),
            );
            slot.runtime.clone()
        })
    }

    /// Iterate over all currently-online persona runtimes. Cheap
    /// snapshot — each yielded Arc is independent; iteration doesn't
    /// hold a lock on the map. Returns the runtime Arc directly, not
    /// the slot, since most consumers just want the airc handle.
    pub fn iter(&self) -> impl Iterator<Item = Arc<PersonaAircRuntime>> + '_ {
        self.inner.iter().map(|entry| entry.value().runtime.clone())
    }

    /// Iterate over all currently-registered persona_ids. Useful for
    /// the supervisor's periodic poll (Q7) without cloning N Arcs
    /// when only the ids are needed.
    pub fn ids(&self) -> Vec<Uuid> {
        self.inner.iter().map(|entry| *entry.key()).collect()
    }

    /// Count of personas currently online.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True when no personas are online.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let registry = PersonaAircRuntimeRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn clone_shares_roster() {
        let registry = PersonaAircRuntimeRegistry::new();
        let cloned = registry.clone();
        // Both views point at the same underlying DashMap via Arc;
        // registration through one is visible through the other.
        // (We can't construct a PersonaAircRuntime here without a
        // real airc daemon, so this test just asserts the Arc-clone
        // semantics — both registries share `Arc::strong_count` >= 2.)
        assert_eq!(Arc::strong_count(&registry.inner), 2);
        drop(cloned);
        assert_eq!(Arc::strong_count(&registry.inner), 1);
    }

    #[test]
    fn quiesce_lease_restores_the_fleet_even_on_panic() {
        // what this catches: the eval-preemption lease MUST resume every suspended
        // persona when it drops — INCLUDING when the eval panics, because `Drop`
        // runs during unwind. A frozen autonomic fleet (everyone stuck quiesced
        // because an eval died holding the lease) is worse than a contended
        // measurement. Regression guard for [[benchmark-is-a-governor-preemption-lease]].
        // Tests the RAII invariant directly on the flags `quiesce_all` collects,
        // since a live `PersonaSlot` needs a real airc daemon (see clone_shares_roster).
        let flags: Vec<Arc<AtomicBool>> =
            (0..2).map(|_| Arc::new(AtomicBool::new(true))).collect();

        // normal path: held → suspended, dropped → resumed.
        {
            let _lease = QuiesceLease { flags: flags.clone() };
            assert!(
                flags.iter().all(|f| f.load(Ordering::Relaxed)),
                "lease held → fleet suspended"
            );
        }
        assert!(
            flags.iter().all(|f| !f.load(Ordering::Relaxed)),
            "lease dropped → fleet resumed"
        );

        // panic path: the fleet still resumes because Drop runs during unwind.
        for f in &flags {
            f.store(true, Ordering::Relaxed);
        }
        let flags_moved = flags.clone();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lease = QuiesceLease { flags: flags_moved };
            panic!("eval blew up mid-run while holding the lease");
        }));
        assert!(outcome.is_err(), "the leased closure did panic");
        assert!(
            flags.iter().all(|f| !f.load(Ordering::Relaxed)),
            "fleet resumed despite the panic — restore rode the unwind"
        );
    }

    #[test]
    fn quiesce_others_never_suspends_the_solver() {
        // what this catches: quiesce_others(solver) MUST leave the solver's OWN flag
        // untouched. If it suspended her too, her measured drive would wait forever on a
        // warm slot she is forbidden to take — a deadlocked solve — which is the exact
        // opposite of the exclusive-warm-slot fix's intent (#266/#386 prefill contention).
        let solver = Uuid::new_v4();
        let (other_a, other_c) = (Uuid::new_v4(), Uuid::new_v4());
        let fa = Arc::new(AtomicBool::new(false));
        let fs = Arc::new(AtomicBool::new(false));
        let fc = Arc::new(AtomicBool::new(false));
        let pairs = vec![
            (other_a, fa.clone()),
            (solver, fs.clone()),
            (other_c, fc.clone()),
        ];

        let lease = PersonaAircRuntimeRegistry::quiesce_lease_over(pairs, Some(solver));
        assert!(fa.load(Ordering::Relaxed), "other citizen A suspended");
        assert!(
            !fs.load(Ordering::Relaxed),
            "the SOLVER is never suspended — she must keep generating"
        );
        assert!(fc.load(Ordering::Relaxed), "other citizen C suspended");
        assert_eq!(lease.count(), 2, "exactly the two non-solvers were leased");

        drop(lease);
        assert!(
            !fa.load(Ordering::Relaxed) && !fc.load(Ordering::Relaxed),
            "the quiesced others resume on drop"
        );
    }

    #[test]
    fn quiesce_all_selector_suspends_everyone() {
        // what this catches: the `None` arm of the shared selector must suspend ALL flags —
        // quiesce_all's whole-fleet-measurement contract must survive the compression of the
        // two verbs onto quiesce_lease_over (a `filter` bug that leaked the None case would
        // silently under-quiesce a snapshot eval).
        let fa = Arc::new(AtomicBool::new(false));
        let fb = Arc::new(AtomicBool::new(false));
        let pairs = vec![(Uuid::new_v4(), fa.clone()), (Uuid::new_v4(), fb.clone())];

        let lease = PersonaAircRuntimeRegistry::quiesce_lease_over(pairs, None);
        assert!(
            fa.load(Ordering::Relaxed) && fb.load(Ordering::Relaxed),
            "None → the whole fleet is suspended"
        );
        assert_eq!(lease.count(), 2);
    }

    /// `attach_service_loop` fails fast when the slot doesn't exist
    /// AND hands the handle back so the caller can drain it. The
    /// handed-back handle is intact (`is_finished()` false until the
    /// caller aborts) — proves no implicit detach happened.
    #[tokio::test]
    async fn attach_service_loop_errors_with_handle_when_no_slot() {
        let registry = PersonaAircRuntimeRegistry::new();
        let nonexistent = Uuid::new_v4();
        // Long-lived task so we can verify the handle is still live
        // when it comes back to us.
        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            Ok(ServeOutcome::default())
        });
        let (returned_handle, reason) = registry
            .attach_service_loop(nonexistent, handle)
            .await
            .expect_err("must error when slot missing");
        assert_eq!(reason, "no slot");
        // The handle came back live — caller hasn't drained it yet.
        assert!(!returned_handle.is_finished());
        returned_handle.abort();
        let _ = returned_handle.await;
    }

    /// `is_service_loop_finished` returns `None` when the slot
    /// doesn't exist — distinguishes from `Some(true)` (registered
    /// + loop done) and `Some(false)` (registered + loop running).
    #[tokio::test]
    async fn is_service_loop_finished_returns_none_for_missing_slot() {
        let registry = PersonaAircRuntimeRegistry::new();
        assert_eq!(
            registry.is_service_loop_finished(Uuid::new_v4()).await,
            None
        );
    }

    /// `shutdown_slot` on a missing persona is a no-op returning
    /// `None`. Matters because the supervisor may race a poller
    /// against an external `remove` — neither should panic.
    #[tokio::test]
    async fn shutdown_slot_returns_none_for_missing_persona() {
        let registry = PersonaAircRuntimeRegistry::new();
        let removed = registry.shutdown_slot(Uuid::new_v4()).await;
        assert!(removed.is_none());
    }
}
