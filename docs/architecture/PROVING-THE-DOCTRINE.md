# Proving the Doctrine — Architecture Tests as Discipline

> Companion to [SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md](SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md).
> The doctrine says WHAT. This doc says HOW WE PROVE IT.

## What this doc is

The substrate doctrine is load-bearing. If we believe it but never
measure it, we have decoration. If we measure each clause with a
test that would fail when the clause is violated, we have an
**organism whose reliability is the union of its proofs** — not a
wish, an evidence trail.

This doc names the five shapes of architecture proof, lays out the
doctrine-clause × proof-shape matrix, and pins the review discipline
that keeps the matrix honest as the substrate grows.

The slogan: **prove it as we build it.** Not "ship the principle then
come back later to validate." Each new primitive ships with at least
one architecture proof for the clauses it depends on. The matrix grows
with the substrate. At the end, the organism's reliability is measured,
not asserted.

## The five proof shapes

Every architectural claim falls into at least one of these. Most need
two or three to be fully proven.

### Shape 1 — Unit-level invariant

**What**: A single test that codifies a doctrine clause as a hard
assertion. Lives in the module's own `mod tests` block (one per file,
per CLAUDE.md test discipline).

**When to use**: For clauses that have a single concrete failure mode
expressible in one input/output check.

**Example**:
`core/continuum-core/src/runtime/command_executor.rs::tests::unknown_command_returns_typed_no_fallback_error_not_ts_attempt`
proves the no-fallbacks doctrine clause. Would fail if anyone
restored the silent TS fallthrough.

**Tag convention**: `// proves: <doctrine clause>` on the test or the
section header above it.

### Shape 2 — Property-based test

**What**: A `proptest`-style harness that varies inputs across a
parameter space and asserts the principle holds for ALL valid
configurations.

**When to use**: For claims that hold across an infinite (or very
large) configuration space — scaling, ordering invariants, composition
identities.

**Example**:
A geometric-scaling harness that varies `(N personas, M topics,
K subscribers per topic)`, emits one event, measures parallel
completion count vs sequential lower bound, asserts the throughput
curve is super-linear. Would fail if someone accidentally serialized
the bus.

**Location**: `core/continuum-core/tests/architecture/flow.rs` and
peers. Tests are integration-tier so they can build their own
substrate under varied configuration.

### Shape 3 — Benchmark-with-assertions

**What**: `criterion`-shaped benchmark that doesn't just MEASURE but
ASSERTS a threshold. The bench fails if the regression dips below a
ratio relative to a baseline.

**When to use**: For performance principles where "we believe X is
faster than Y" needs to become "we have numbers, and CI catches it
when we regress."

**Example**:
A bench that runs the vision pipeline with 0 subscribers vs 1
subscriber. Asserts: with 0 subscribers, CPU time ≤ ε (idle).
With 1 subscriber, CPU time = baseline ± tolerance. Linear
correlation as subscribers grow. Would fail if a future change
broke subscriber-gated production.

**Location**: `core/continuum-core/benches/architecture/`. The
benches double as both perf regression catchers AND principle
proofs.

### Shape 4 — Adversarial / chaos test

**What**: A test that simulates misbehavior, then asserts the
substrate refuses, captures, audits, and recovers — never silently
fails.

**When to use**: For claims about alignment, fault tolerance,
federation, and "structural" safety properties. The only way to
prove "it won't go dark under hostile input" is to FEED IT HOSTILE
INPUT and watch what happens.

**Example**:
A test that spins up a substrate node + a "malicious peer" via
`TwoAircLoopback`. Malicious peer emits malformed events, claims
identities it doesn't own, attempts to dominate sentinel quorum.
The substrate's `AuthPolicy` gate refuses; the audit captures
every attempt; sentinel verdict consensus rejects. Test asserts
ALL of: (a) substrate didn't crash, (b) malicious events didn't
reach business logic, (c) audit trail is complete, (d) sentinel
verdicts published with reasoning.

**Location**: `core/continuum-core/tests/architecture/adversarial/`.

### Shape 5 — Build-graph constraint

**What**: A constraint enforced by the compiler or by a CI lint that
the dependency graph (or the public API surface) matches the
architecture. No runtime needed — the rule is verified at build
time.

**When to use**: For layering principles, public-API closure,
forbidden import patterns, "no `OnceLock<Arc<Global>>` in this
crate," etc.

**Example**:
A custom check (or a `cargo-deny` rule) that
`continuum-core::cognition::*` does not depend on
`continuum-core::runtime::*` private items. Build-fails if anyone
adds a `use crate::runtime::internal::*` from a cognition module.
The engine-on-OS layering becomes structurally enforced.

**Location**: `core/.cargo/check_arch.rs` (or similar custom check),
`deny.toml`, or `tests/architecture/layering.rs` using
`compile_fail` markers.

## The doctrine-clause × proof-shape matrix

Each row is a clause from [SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md](SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md).
Each cell is the shape(s) of proof covering it, with the location
of the proof OR the issue/PR tracking its creation.

Status legend: `✅` proven  `🟡` partial (one shape covers, others
needed)  `🔴` not yet proven, work item

| Doctrine clause | Shape 1 (unit) | Shape 2 (property) | Shape 3 (bench) | Shape 4 (adversarial) | Shape 5 (build) | Status |
|---|---|---|---|---|---|---|
| No silent TS fallthrough | `command_executor::tests::unknown_command_returns_typed_no_fallback_error_not_ts_attempt` + `tests/architecture_no_fallbacks.rs` (3 fns: single-command, structural-across-shapes, positive-path-preserved) | — | — | — | (could add: forbidden import of TS socket from non-bridge modules) | ✅ |
| Engine-OS layering (cognition doesn't depend on runtime internals) | — | — | — | — | `tests/architecture_engine_os_layering.rs` (ratchet: 29 grandfathered, NEW violations BLOCK) | 🟡 |
| Localized state per citizen (no singleton substrate state) | per-module field type check | — | — | — | `tests/architecture_no_singleton_state.rs` (ratchet: 12 grandfathered `static OnceLock<Arc<T>>` / `OnceCell<Arc<T>>` singletons; NEW ones BLOCK; positive test pins `LateBound<T>` at its canonical location) | 🟡 |
| `PerKeyGate` structural eviction (no leak) | `training_trigger::tests::submit_at_threshold_dispatches_and_clears` (asserts `submit_gates.len() == 0`) | `per_key_gate::tests::stress::concurrent_acquire_drop_cycles_actually_serialize_critical_sections` | — | — | — | ✅ |
| `LateBound<T>` install-once semantics | `late_bound::tests::second_install_is_silent_noop_original_wins` | — | — | — | static_assert `Send + Sync` | ✅ |
| Flow scales geometrically (events > RPC under N consumers) | — | TODO: proptest over `(N, M, K)` | TODO: criterion event-fanout vs RPC-equivalent | — | — | 🔴 |
| Demand-pull eliminates idle work | — | — | TODO: bench vision encoder CPU = 0 with no subscribers | — | — | 🔴 |
| Backpressure is intrinsic (no unbounded queue growth) | — | TODO: proptest slow-consumer scenarios | — | `tests/architecture_backpressure_chaos.rs` (2 fns: `flooding_producer_surfaces_typed_lag_to_slow_consumer`, `consumer_makes_progress_after_lag` — proves bounded queue, typed `LiveLag` signal, no silent loss, recoverable subscription) | — | 🟡 |
| Federated alignment (hostile peer cannot dominate) | — | — | — | `tests/architecture_federated_alignment.rs` (2 fns: `hostile_peer_dispatch_is_refused_with_typed_forbidden_verdict`, `gate_sees_callers_airc_verified_peer_id_not_a_claimed_one` — proves typed `Verdict::Forbidden` short-circuits at the gate; airc-verified peer_id flows into AuthPolicy, can't be header-claimed) | — | 🟡 |
| Every scorer has VDD baseline + ML upgrade path | per-scorer baseline test | — | — | — | TODO: registry walk asserting every scorer has `Baseline + MlCandidate` impls | 🔴 |
| Sentinels observe at zero added latency | — | — | TODO: bench main-flow latency with 0 vs N sentinels | — | — | 🔴 |
| Cross-grid composition (peer subscriptions work over airc) | `tests/integration/airc_remote_inference_roundtrip.rs` (existing) | — | — | `tests/architecture_cross_grid_chaos.rs` (2 fns: silent-peer-timeout-surfaces-typed-error, transport-remains-callable-after-timeout — also caught + fixed a real classifier bug in `AircLiveTransport::send_request`) | — | ✅ |
| `Observability is half the substrate` (every load-bearing decision capturable) | per-module CaptureSink test | — | — | — | TODO: build-graph check that load-bearing modules implement the capture trait | 🟡 |
| Module compose-by-event (no Commands.execute inside substrate-internal logic) | — | — | — | — | `tests/architecture_compose_by_event.rs` (ratchet: 1 grandfathered `executor.execute_json` in `cognition/vision_describe.rs`; NEW BLOCK; migration tracked under #112-#114, graduates to ✅ when ratchet hits 0) | 🟡 |
| RAII Lease drop ordering (PerKeyGate doctrine v2) | `per_key_gate::tests::lease_drop_evicts_gate_when_no_other_holders` + `gate_survives_first_lease_drop_while_second_holds` | — | — | — | `#[must_use]` on `Lease` | ✅ |

The matrix has gaps. Gaps are the substrate's next architecture-test
PRs.

## How to add a new proof

1. **Identify which clause + shape.** Read the doctrine doc, pick the
   clause you're proving (or adding). Pick the shape from above.
2. **Write the test.** Conventional location per shape.
3. **Tag it with `// proves: <clause>`.** This makes the matrix
   self-auditing — `git grep '// proves:'` walks every proof.
4. **Update the matrix in this doc.** One-line edit. PR review
   catches stale rows.
5. **Cite the clause in the PR body.** "This PR adds shape-2 proof
   for the `flow scales geometrically` clause."

## How to add a new clause

If you find an architecture claim that isn't covered by an existing
clause:

1. **Add it to [SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md](SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md).** The
   doctrine doc is where principles live.
2. **Add a row to this matrix.** All proof cells start at `🔴`.
3. **Write at least one proof in the same PR.** Don't merge a clause
   without a proof. Otherwise the doctrine grows wishful and the
   matrix grows red.

## Review discipline

For PRs that touch substrate territory:

- **PR body references the matrix.** "This PR's changes are covered
  by clauses X and Y; existing proofs hold; this PR adds proof Z."
- **Failing architecture test = doctrine violation = BLOCK.** Same
  as failing unit tests. Reviewers check that any red cell that
  the PR's surface touches has at least a plan attached.
- **New primitive without a proof = nit (review BLOCK on big
  primitives).** Small primitives can ship with the proof in a
  follow-up; load-bearing ones (PerKeyGate, LateBound, etc.) ship
  with the proof or don't ship.
- **Stale matrix = doc bug.** Reviewer can grep `// proves:` and
  cross-reference against this doc. Drift caught at PR time.

## The `tests/architecture/` layout

```
core/continuum-core/tests/architecture/
├── flow.rs              ← shape 2: geometric scaling, fanout fairness
├── demand.rs            ← shape 2 + 3: idle work, backpressure
├── scorers.rs           ← shape 2: every scorer VDD-conforms
├── federation.rs        ← shape 4: adversarial peer scenarios
├── layering.rs          ← shape 5: build-graph + compile_fail
├── observability.rs     ← shape 5: every load-bearing decision capturable
└── README.md            ← points back at this doc
```

Each file's tests carry `// proves: <clause>` tags. Grep is the audit.

## What "prove it as we build it" means in practice

- The doctrine doc is the **lighthouse** — direction.
- The matrix is the **scoreboard** — coverage.
- The `tests/architecture/` tree is the **scaffold** — actual proofs.
- Each PR is a **stone** — small, verifiable, increments the matrix.

The organism's reliability at any moment is the union of green cells
in the matrix. Red cells are visible debt. The substrate cannot lie
about itself because every doctrine clause is interrogable.

This is the discipline that scales the doctrine without scaling the
risk. It's also what makes the substrate auditable from outside — a
sentinel (or a human reviewer) can ask "is the federation alignment
clause proven?" and the matrix answers. No politics, no narrative,
just the matrix and the tests it points at.

## Cross-references

- [SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md](SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md) — the WHY
- [CONCURRENCY-STYLE-GUIDE.md](CONCURRENCY-STYLE-GUIDE.md) — the runtime shape
- [OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md) — the capture trait
- CLAUDE.md § "test infrastructure" — TDD/VDD conventions, one
  `mod tests` per file, `#[cfg(feature = "stress-tests")]` for
  multi-thread stress
- `[[vdd-math-accuracy-doctrine]]` — the VDD discipline applied to
  numerical accuracy; this doc is the doctrine-level analog

---

*Created 2026-06-09. The matrix above is a starting snapshot. PRs
that add or remove proofs MUST update the matrix in the same
commit. Drift between doc and reality is the bug class this doc
exists to prevent.*
