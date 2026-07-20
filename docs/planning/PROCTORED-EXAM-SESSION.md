# Proctored Exam Session — a dependable benchmark serving process

**Status:** planned (2026-07-20). Approved direction: build a dependable *process*, not a
timeout-hacked script. Author-once, run-anywhere; the same result every time because it
**verifies** rather than **waits-and-hopes**.

> Joel: "It needs to be a well organized process not some simple script where you just
> increase a timeout. Make it dependable."

---

## The problem (glass-boxed 2026-07-20)

A living-persona benchmark (`benchmark/run` / `cognition/eval` with no gene, no
`base_model_id` — the `(None, None)` branch) runs the persona ON the **shared serving
lane** (`:58057`). That lane is **not a dependable exam target**: under combined load
(N live personas + the exam slot + memory pressure) it flips **not-ready** via one of
three axes:

1. **grow-back relaunch** — the plan wants a bigger window → relaunch → in-flight
   generations connection-refused. *(Addressed: steady-hold, `25fc1dc51`.)*
2. **#175 decode-failure self-heal** — concurrent-worst-case KV overflows the served
   window at N full slots → Metal compute error → self-heal flips not-ready + relaunches.
3. **pressure shrink / cold-after-reboot** — the lane transitions and is briefly
   `serving: <none>, ready: false`.

**The dependability bug (the real one):** when the lane is not-ready, each task's
generation is refused (`model 'X' is not the active served model … ready: false`), and
the run reports **`passRate: 0.0`** — a **fake zero indistinguishable from "she got it
wrong."** Across every hard-rs run this session: `outTokens: 0`, `decode 0 tok/s`, always
an infra cause, never her coding. A benchmark that can't tell *"she scored 0"* from
*"the harness never gave her a working lane"* is untrustworthy by construction. No timeout
tuning fixes that.

---

## Already built this session (the foundation — DO NOT rebuild)

- **`fix/lane-relaunch-connect-retry`** (`edd71bfda`): a CONNECT to a mid-relaunch local
  lane retries the same lane (bounded) instead of faulting the turn. Handles a *transient*
  bounce; not a *sustained* not-ready.
- **`feat/lane-admission-planner`** (`aed37a383`, `765b1993d`): `resources/placement.rs`,
  the pure model-aware admission kernel:
  - `plan_placement(capacity, resident, demand)` → `ShareLane` (same base resident ⇒
    co-tenant slots, no 2nd weight copy) | `SpawnLane{reclaim}` (fresh copy, tier lower
    tiers down first) | `CpuSpill`. Tiers: `Live > Eval > Background`.
  - `plan_grid_placement(nodes, demand)` — same kernel across reachable nodes, affinity
    first (route to the node that already has the weights WARM), with join/leave/partition.
  - 9 scenario tests on the real incident geometry (Devstral-24B Q4). All green.
- **steady-hold** (`25fc1dc51`): `ServingSteadyHold` RAII in `serving_daemon.rs`; while an
  eval holds it, `reconcile_to_plan` skips the *grow-back re-home* for an already-correctly-
  served lane (does NOT suppress a model change or a pressure shrink). `eval.rs` `(None,None)`
  acquires it. Unit-tested (refcounted RAII).

The steady-hold fixed axis (1). Axes (2) and (3) still flip the lane not-ready, and the
eval still reports a fake `0`. That's what this plan finishes.

Branch: `feat/lane-admission-planner` (also carries the retry + steady-hold). Sibling
memory: `[[lane-admission-planner-scenario-driven]]`, `[[benchmark-needs-its-own-serving-lane]]`,
`[[benchmark-is-a-governor-preemption-lease]]`, `[[llama-compute-error-wedge-is-per-slot-context-overflow]]`.

---

## The design: a Proctored Exam Session (a lifecycle owned by the substrate)

Four phases, deterministic, fail-loud, repeatable. NOT a shell wrapper.

1. **ACQUIRE a verified serving context** — via `plan_placement`. Build a `LaneDemand`
   (her base + genome + an *exam-adequate* per-slot window + `Eval` tier) and the resident
   set from the live serving snapshot. Decide:
   - `ShareLane` **and** the live per-slot window is adequate for exam prompts → pin it
     (steady-hold).
   - window too small / lane contended → **preempt**: tier live serving down (fewer lanes /
     smaller window / pause background personas) to free a stable slot.
   - can't fit even after preemption → `CpuSpill` (slow but *stable*).
   Return the handle **only when the lane is decode-verified ready** — a real one-token
   decode, not `serving/status` optimism and not `/health` 200.
2. **HOLD it stable for the whole exam** — steady-hold generalized: no grow-back relaunch,
   AND the removed load means no decode-overflow self-heal churn (axis 2 gone), AND an
   adequate window is pinned (axis 3 gone).
3. **RUN with infra-faults ≠ task-failures** — the keystone. A per-task
   `not the active served model` / connect-refused / compute-error is an **infra fault**:
   pause, re-verify the context, retry the task (bounded). If the context can't be restored,
   the run **aborts as `InfraUnavailable(reason)`, never `passRate: 0.0`**. A `0` can only
   mean she was given a verified lane and got the answer wrong.
4. **RELEASE** — restore live serving (RAII on the handle drop).

---

## Slices (build order)

### Slice B — trustworthy result (keystone, FIRST) — ✅ LANDED (`f25c76087`)
Make the *number* trustworthy even before preemption exists.
- Eval result becomes an outcome: `Scored { pass_rate, … } | InfraUnavailable { reason, tasks_attempted }`.
  `benchmark/run` + `cognition/eval-status` surface it; a fake `passRate: 0.0` on an
  unverified lane is now impossible.
- The eval run loop classifies each generation failure: **infra fault** (not-ready /
  connect-refused / "not the active served model" / compute-error / stream-idle timeout)
  vs **task failure** (a real graded-wrong answer). Infra fault → re-verify + bounded retry
  of that task; exhausted → abort `InfraUnavailable`.
- **Verified-ready gate**, applied to the SHARED-lane path too (today only the ephemeral
  lane decode-verifies at spawn): decode-verify before task 1 AND re-verify between tasks.
- Files: `cognition/eval.rs` (run loop + result type + warm-gate region), the
  `deliberation inference failed` classification seam, `cognition/eval-status` row,
  `benchmark/run` result.
- Test: fault-injection — a lane that flips not-ready mid-run yields `InfraUnavailable`,
  never `passRate 0.0`; a genuinely-wrong answer on a verified lane yields `Scored` with the
  0 counted.

**As landed (`f25c76087`):** `PassOutcome { pass, results, infra_faults, infra_reason }`
replaces the bare `(pass, results)` tuple from `run_pass`/`run_pass_team`. On an
`inference_error`, `run_pass` re-verifies the lane (`await_ready_serving`) and retries the
SAME task up to `INFRA_FAULT_RETRIES=3` (the retried generation IS the decode proof); an
unrecoverable fault records the row, marks the run void, and ABORTS the loop.
`infra_verdict(&PassOutcome) -> Option<InfraUnavailable>` (pure, one decision point) drives
the new `CognitionEvalResult.infra_unavailable` field → ledger row `infraUnavailable`,
`BenchmarkRunResult.infra_unavailable`, and a LEARN-mode guard (never learn from a dead-lane
run). The old grading-match `inference_error` arm is deleted — an infra fault can no longer
be graded as a miss. Two fault-injection unit tests pin both halves. NOT yet done in B:
the full re-verify+retry inside team mode (it classifies + aborts) and the A/B two-arm path
(runs on an EphemeralServingLane that decode-verifies at spawn) — both scoped follow-ups.

### Slice C — adequate window (kills the churn source, axis 2)
The served window must satisfy **concurrent-worst-case**:
`window × active_slots × kv_per_token ≤ free_after_weights`. Today grow-back sizes for
empty slots, so adding the exam slot (or an N-persona LiveKit huddle) overflows KV → the
decode-failure self-heal that flips the lane not-ready. Fixing this removes the main reason
the shared lane isn't stable under an exam.
- Files: `cognition/serving_plan.rs` (window sizing), `serving_daemon` reconcile.
- Test: extend `serving_plan` tests with the concurrent-worst-case invariant (mirrors
  `placement::huddle_counts_kv_for_every_concurrent_slot`).

### Slice A — dependable context via preempt (the strongest guarantee)
`ExamServingContext::acquire(persona, demand) -> Result<Handle, Reason>` executing the
`plan_placement` decision: acquire steady-hold and/or drive the preemption (tier live
serving down via the existing `serving_tier_down::CatalogTierDownPolicy` +
`governor.reconcile_for_demand` + `PlannedReclaim`) to free a stable slot, bring the lane
to decode-verified ready, return the handle only then; restore on drop. This is
`[[benchmark-is-a-governor-preemption-lease]]`, and it generalizes to the grid via
`plan_grid_placement` (route the exam to a peer that already has the weights warm, or a
peer with headroom, before preempting the local live lane).
- Files: new `cognition/proctored_exam.rs` (the lifecycle) or extend `cognition/eval.rs`;
  `serving_daemon` (a tier-down-to-target + restore API); `resources/placement` wiring.
- Test: scenario tests reusing the placement oracle (share-adequate → pin; window-too-small
  → preempt to 1 lane; contended → preempt background not live; can't-fit → cpu-spill).

**Order:** **B → C → A.** B makes the result trustworthy immediately. C removes the main
instability (decode-overflow self-heal). A gives the guaranteed-stable dedicated context and
is the grid-general answer. After C, re-measure whether the shared lane is stable enough
that A is only needed under heavy live load / on the grid.

---

## Validation (dependable, NOT a script)

- Unit/scenario tests per slice — the oracle. `capacity/sim.rs` is the deterministic gym for
  the preemption/placement decisions (`[[capacity-fabric-live-never-block-sim-as-gym]]`).
- The single native path IS the process: `cu benchmark/run --name hard-rs` returns either a
  real `Scored` number **on a verified lane**, or a loud `InfraUnavailable` reason — no bash
  timing, no "sleep then hope." That command being dependable is the acceptance test.
- Must NOT regress live personas: preemption tiers them down gracefully (they keep
  answering, slower) and restores after. Gated so ONLY a real proctored exam triggers it
  (`[[first-class-citizens-even-during-benchmarks]]`, `[[benchmarks-are-proctored-exams-of-the-natural-living-persona]]`).

---

## The curriculum tie-in (why this matters beyond one number)

Every ACQUIRE decision (`plan_placement` verdict + the live demand/resident context) and its
outcome (Scored / InfraUnavailable / preempted-what) is a structured, capturable record. The
patterns are predictable (persona rhythms, scheduled benchmarks, known huddle rosters), so
these records become the training corpus for a learned `GridPlacementPolicy` /
`AllocationPolicy` (`Score` scalar, `[[grid-agreements-swappable-policy-deterministic-rails]]`,
#103) that beats the deterministic baseline in the sim. Build the lifecycle so the decision +
outcome are emitted as a capture record from the start (`[[observability-as-substrate]]`).
```
