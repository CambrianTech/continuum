# Production Cognition Replay — From PROD, Not POC

> **Premise** (Joel, 2026-05-18): *"We need 100% Rust cognition sooner rather than later and proof it works. Solid recording and replay of persona, FROM PROD, not just dummy proof of concepts these guys always rig up. They need to up their game."*
>
> **Status.** Spec for the prod-validation loop. Implementation lands per ALPHA-GAP Lane D + the next-tier cognition modules (persona-cognition, inference-llm, composer, speculator).
>
> **Companion to** [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) (defines `TurnReplayRecord`), [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) (the trace bus this record rides on), [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) (sentinel-AI consumes these records for attribution), and [PERFORMANCE-HARNESS-FRAMEWORK.md](PERFORMANCE-HARNESS-FRAMEWORK.md) (replay harnesses are a category there).

## Why This Doc Exists

The substrate has shipped end-to-end in Rust over the last 48 hours: governor, working-set-manager, demand-aligned-recall, audit-recorder, check_redundancy oxidation. ~25+ PRs of substrate work in canary.

**None of it has been validated against production traffic.** The TurnReplayRecord type exists; no production turn has been recorded. The chat-roundtrip-live-harness exists; it consumes `RuntimeFrame::synthetic_chat("hello")` — a synthetic fixture, not a captured real turn. Tests pass; demos work; whether the substrate behaves correctly on what real personas actually do under real load — **we don't know.** That's the gap.

> *"these guys always rig up"* — Joel naming the failure mode: a working demo that doesn't survive contact with production. This document specifies the loop that closes it.

The architectural answer is a **production-recording → deterministic-replay → bit-equal-validation** loop, where every persona turn in production:

1. **Produces a signed `TurnReplayRecord`** with cryptographic provenance + full input/output state.
2. **Lands in a tamper-evident archive** that survives substrate restarts.
3. **Can be replayed** against the current substrate code with deterministic-identical output, or fails loud with a typed `ReplayDivergence`.
4. **Is consumed by sentinel-AI** for outcome attribution + the validation harnesses for regression detection.

If any of those four steps is missing, we don't have "100% Rust cognition with proof." We have substrate-shaped scaffolding.

## The Four Substrate-Enforced Properties

Production replay is structural. It is not a "QA process." It is a property the substrate proves for every turn:

### Property 1 — Every Turn Produces A Signed TurnReplayRecord

The persona-cognition module's `handle_frame` returns only after the substrate has signed + persisted a `TurnReplayRecord` for that turn. Per `PERSONA-COGNITION-CONTRACT.md` §"Core Surfaces" → §"`TurnReplayRecord`":

```rust
pub struct TurnReplayRecord {
    pub turn_id:           TurnId,
    pub persona:           PersonaId,
    pub frame:             Arc<RuntimeFrame>,
    pub assembly:          WorkingMemoryAssemblySnapshot,
    pub recall_trace:      RecallTrace,
    pub lease:             CognitionLeaseSnapshot,
    pub composition:       CompositionPlanSnapshot,
    pub decision:          PersonaDecision,
    pub output:            Option<RenderedOutput>,
    pub timing:            TurnTiming,
    pub resource_usage:    ResourceUsage,
    pub provenance_chain:  Vec<ArtifactRef>,
    pub signature:         TurnSignature,
}
```

**Substrate enforces this by type.** The `persona-cognition` module's `handle_frame` returns `ModuleResult::Ok` only after the record is signed and the signature verified. A turn that fails to produce a record fails the substrate's invariant test — it is a substrate bug, not an optional feature.

### Property 2 — Records Persist To A Tamper-Evident Archive

Records land in `~/.continuum/replay/<turn_date>/<turn_id>.jsonl` as one signed line per turn. The directory rolls daily. The substrate's `replay-archive` module owns:

- Append-only write semantics (same shape as audit-recorder #1344).
- Per-turn signature verified at write time and again at read time.
- A chain-hash linking turns in temporal order so a missing turn is detectable.

Records are persona-private by default — only the producing persona's identity can read its own records. Federation (cross-instance sharing of replay records) requires explicit consent + provenance, same shape as sentinel artifact sharing in `GENOME-FOUNDRY-SENTINEL.md` §10.

### Property 3 — Deterministic Replay Against Current Substrate

A `cargo replay <turn_id>` invocation:

1. Loads the record from the archive.
2. Reconstructs the substrate state needed for replay: composition pinned, recall index snapshotted, governor policy at the record's `policy_version`, persona's `IdentityStateSnapshot` restored.
3. Re-runs the persona-cognition module against the recorded `RuntimeFrame`.
4. Produces a *new* `TurnReplayRecord` from the replay.
5. Compares structured fields bit-equal against the original.

```rust
// PROPOSED — src/workers/continuum-core/src/cognition/replay/mod.rs
pub trait CognitionReplayer: Send + Sync {
    /// Replay a recorded turn deterministically. Returns the replayed
    /// record; comparison is the caller's job (the harness layer).
    async fn replay(&self, record: &TurnReplayRecord) -> Result<TurnReplayRecord, ReplayError>;

    /// Verify a record's signature + provenance chain. Pure function.
    fn verify(&self, record: &TurnReplayRecord) -> Result<VerifiedRecord, VerificationError>;

    /// Bit-equal field comparison. Returns a typed diff when they
    /// don't match — the diff IS the bug report.
    fn diff(&self, original: &TurnReplayRecord, replayed: &TurnReplayRecord) -> ReplayComparison;
}

pub enum ReplayComparison {
    BitEqual,
    Divergence { fields: Vec<DivergedField>, severity: ReplaySeverity },
}

pub enum ReplaySeverity {
    /// Output differs but the decision is the same and the substrate
    /// can prove the difference is bounded reprojection (e.g. recall
    /// scored slightly different on a non-determined tiebreak). Logged,
    /// not failed.
    BoundedNonDeterminism,
    /// Output differs in a way that crosses a decision boundary
    /// (Speak vs Decline, or different addressee). FAILS the replay
    /// harness; PR cannot merge without explanation.
    DecisionBoundaryCrossed,
    /// Substrate state mismatch (governor policy version, working set
    /// composition, etc.) — environmental drift, not a cognition bug.
    /// Logged + flagged; harness rerun after substrate stabilizes.
    SubstrateStateDrift,
}
```

### Property 4 — Sentinel + Harnesses Consume Records From Prod, Not Synthetic

Two downstream consumers are explicitly bound to the replay archive:

- **Sentinel-AI's attribution loop** (per `GENOME-FOUNDRY-SENTINEL.md` Part 6) reads from `~/.continuum/replay/`. It does not consume synthetic test fixtures. If the replay archive is empty, sentinel has nothing to attribute and emits a typed `NoTracesYet` signal — explicit, not silent.
- **Validation harnesses** (per `PERFORMANCE-HARNESS-FRAMEWORK.md`) have a Tier-1 entry `prod-replay-harness` that consumes a directory of captured records and asserts bit-equal reproduction. The harness fails the PR if any record's replay produces a `DecisionBoundaryCrossed` divergence.

`prod-replay-harness` is what closes the "POC vs PROD" gap. The chat-roundtrip-live-harness from #1348 uses synthetic frames because nothing else existed yet. `prod-replay-harness` uses real captured records. Both ship; both are Tier 1; the prod one is the load-bearing acceptance gate.

## The Capture-Then-Replay Loop, End To End

```text
PRODUCTION RUN — every turn

   Activity emits RuntimeFrame
            │
            ▼
   Persona-cognition module wakes
            │
            ▼
   ... (assembly, recall, composition, decision) ...
            │
            ▼
   Substrate signs TurnReplayRecord  ◄─── Property 1 enforced here
            │
            ▼
   replay-archive.append()           ◄─── Property 2 enforced here
            │
            ▼
   Persona's PersonaDecision emitted

──────────────────────────────────────────────────────────────────

REPLAY — deterministic, repeatable

   cargo replay <turn_id>
            │
            ▼
   Load TurnReplayRecord from archive  ◄── verify signature + chain
            │
            ▼
   Reconstruct substrate state (policy, working set, identity)
            │
            ▼
   Re-run persona-cognition against the recorded frame
            │
            ▼
   New TurnReplayRecord produced
            │
            ▼
   diff(original, replayed) → ReplayComparison
            │
            ▼
   BitEqual → pass         ◄─── Property 3 satisfied
   Divergence → typed failure with severity
            │
            ▼
   Bounded non-determinism: log + continue
   Decision boundary crossed: FAIL the harness, block the PR
   Substrate state drift: log + rerun after stabilization

──────────────────────────────────────────────────────────────────

SENTINEL ATTRIBUTION

   Sentinel-AI reads replay archive
            │
            ▼
   Per turn, attribute outcome to composition artifacts
            │
            ▼
   Refined LoRA layers / engrams / routing tables published
            │
            ▼
   Demand-aligned-recall picks them up via score upgrade

──────────────────────────────────────────────────────────────────

VALIDATION HARNESS

   prod-replay-harness reads N records
            │
            ▼
   Replay each
            │
            ▼
   Tally: BitEqual / Bounded / Boundary / Drift
            │
            ▼
   PR passes if BitEqual + Bounded only
   PR fails if any Boundary
   PR flagged for substrate review if Drift
```

Every step typed. Every transition observable. Every divergence has a named severity that the substrate enforces — never a silent "looks close enough."

## Capture Discipline

The capture side has rules the substrate enforces structurally, not by convention:

1. **No synthetic-fixture path produces TurnReplayRecord.** Test scaffolds may construct `RuntimeFrame::synthetic_*()` fixtures, but the `persona-cognition` module produces signed `TurnReplayRecord`s ONLY when invoked in the production module-loop. Synthetic-test runs do not write to `~/.continuum/replay/`. This prevents the failure mode where the archive fills with synthetic records and replay-harness "passes" against fake data.

2. **Sampling is configurable but defaults to 100%.** Production environments capture every turn. High-volume deployments may sample (e.g. 1-in-10) via governor policy; the sampling decision is itself a substrate-recorded event. Per-persona consent applies; a persona can opt out of capture entirely, in which case its turns produce no records and replay-harness skips them with an explicit `NotCaptured` entry.

3. **Privacy isolation is structural.** A persona's records are persona-private by default. Cross-persona read requires explicit consent (same shape as engram sharing in `PERSONA-COGNITION-CONTRACT.md` §"Compartmentalization"). Sentinel-AI has training-input consent on by default but can be revoked per-persona without breaking the rest of the loop.

4. **Records are content-addressable.** `turn_id` is the content hash of `(persona, frame_id, signature)`. Two captures of the same logical turn (e.g. from a federation peer replaying) collide deterministically — no duplicates, no silent overwrites.

## Replay Discipline

The replay side similarly enforces:

1. **Substrate-state reconstruction is faithful or refused.** Replay must reconstruct: governor policy at `record.policy_version`, working-set tier sizes per the recorded `cascade_step`, composition pinning per `record.composition`. If the policy_version is unknown to the local substrate (e.g. the production substrate was on a policy revision local doesn't have), replay returns `ReplayError::PolicyVersionUnknown` — never proceeds with a substituted policy.

2. **Recall index is snapshotted, not regenerated.** The recall trace in the record names the artifacts that scored above threshold at production time, with their scores. Replay loads the same artifacts (by content hash) — if any have been retired in the meantime, replay returns `ReplayError::ArtifactRetired { artifact, retired_at }` with the audit trail. This catches the failure where "replay passes" only because the substrate has evolved away from the original state.

3. **Determinism boundaries are named.** Some sources of non-determinism are intrinsic to the substrate (parallel embedding generation order, tie-breaking when recall scores match). The replay comparison knows about these and admits `BoundedNonDeterminism` for the documented set — but ANY deviation outside that set is `DecisionBoundaryCrossed` or worse.

4. **Replay is the inverse of capture in cost.** Capture is sub-ms (signing + append). Replay is bounded by the original inference cost; a 5-second cloud LLM turn replays in roughly the same wall-clock. Validation harnesses bound their run by either a turn count (N=100 records) or a wall-clock budget (30 minutes), not by "all of them," so the prod-replay-harness is feasible to run on every PR.

## Acceptance Criteria

The prod-cognition-replay loop is "done" when the following are provable on canary, with PR-attached evidence:

**Capture side:**

- `persona-cognition` module produces signed `TurnReplayRecord` for every turn invoked through the production path. Verified by a regression test that asserts: N synthetic turns produce 0 records (synthetic path is dead); N production-path turns produce N records.
- `~/.continuum/replay/<date>/*.jsonl` exists, append-only, with chain-hash linking.
- Cross-persona read attempt returns `AccessDenied` with audit trail.

**Replay side:**

- A `cargo replay <turn_id>` invocation reproduces the original record bit-equal in the structured-fields domain (the `decision` variant + `output` text + `recall_trace` artifact set + `composition` LoRA stack + `provenance_chain`).
- A tampered record's signature fails `verify` with typed reason.
- A record referencing a retired artifact returns `ArtifactRetired` not a silent substitution.

**End-to-end validation:**

- `prod-replay-harness` is added to `PERFORMANCE-HARNESS-FRAMEWORK.md` as Tier 1. Each PR-relevant Rust change runs the harness against a baseline set of N captured production records. Any `DecisionBoundaryCrossed` divergence fails the PR.

**Sentinel integration:**

- Sentinel-AI reads from the replay archive (not from synthetic fixtures). Demonstrated by a smoke test that empties the archive and observes sentinel emitting `NoTracesYet`; populating the archive then observing sentinel begin attribution within one consolidation cycle.

## Why This Earns Its Space

A 25-PR substrate landing is impressive volume but it's substrate scaffolding. Without prod-replay, every claim about the substrate's behavior is "the tests say so." With prod-replay:

- A persona that drifted in production this week is reproducible on a developer's machine bit-for-bit, deterministically, in seconds.
- Sentinel-AI's "refined LoRA layer X improved outcomes" claim is checkable against real turn-by-turn evidence, not a synthetic benchmark.
- A regression that ships to canary trips the replay-harness before it can poison main.
- The validation gap that calls *"these guys always rig up"* a fair characterization is closed by structural enforcement, not by adding QA process.

This is what 100% Rust cognition + proof it works looks like as substrate, not as audit findings: the substrate produces the evidence on every turn, the substrate stores the evidence safely, the substrate replays the evidence on demand, the substrate fails loud when replay diverges. No human in the loop until a divergence fires.

## Open Questions

1. **Sampling under high load.** Default 100% capture is correct in development; in a high-volume deployment (1000+ turns/min/persona) the archive's I/O cost matters. Tentative: governor sets a sampling rate per cascade step; under cascade 0, 100% capture; under cascade 2+, sample 1-in-10 with explicit `Sampled` markers in the records that did capture so replay-harness skips the missing ones with audit, not silently.

2. **Replay archive size growth.** A persona doing 100 turns/day for a year produces ~36,000 records. JSONL with full RuntimeFrame snapshots is on the order of 1-10 KB per record → ~36-360 MB/persona/year. Tentative: roll daily; archive month-old days to `replay-cold/` with content-hash dedup; never delete (records are evidence; deletion is a substrate operation that emits its own audit record).

3. **Cross-substrate-version replay.** A record produced on substrate v1.0 replayed against substrate v2.0 — how do we tell the difference between "substrate genuinely diverged" and "v1.0 was correct, v2.0 is the bug"? Tentative: the record's `policy_version` includes the substrate's git commit at capture time; replay carries that as a flag; the replay-harness's `SubstrateStateDrift` severity is what surfaces it. A human reads the divergence and decides.

4. **Capture during sentinel refinement passes.** Sentinel produces a new artifact mid-day; the next persona turn uses it. The replay record names the artifact by content hash. A week later sentinel publishes another refinement supersedng it. Does replay use the old hash (which still exists, archived) or the latest? Tentative: replay always uses the exact hash named in the record. If sentinel retired the old artifact, replay surfaces `ArtifactRetired` with the retirement timestamp and the user decides whether to pull the cold copy from archive.

5. **Federated replay-records.** A peer instance produces records; can our instance replay them locally? Tentative: yes, but only if the producing peer's signed substrate version is in our compatible-version set. Replay across substrate variants needs explicit substrate-compat-class declaration (out of scope for v1).

6. **The "always rig up" failure mode the substrate must structurally prevent.** Joel called this out: implementers ship a working demo that doesn't survive production. The substrate's structural answer: synthetic-fixture path produces 0 records → replay-harness has no fake data to "pass" against → "looks good in demo" cannot be confused for "works in prod." But that depends on the synthetic-fixture path actually being disconnected from the record-write path. Tentative test: build a synthetic chat turn through every test scaffold; assert the replay archive is empty after. Failing this test means a synthetic-record leak that would re-open the gap.

## See Also

- [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) §"TurnReplayRecord" — the record shape this document operates on.
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) §"Standard VDD Record" — adjacent record format for performance evidence.
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) §6 — sentinel-AI consumes records from this archive.
- [PERFORMANCE-HARNESS-FRAMEWORK.md](PERFORMANCE-HARNESS-FRAMEWORK.md) — `prod-replay-harness` is added to its Tier 1 catalog.
- [MODULE-CATALOG.md](MODULE-CATALOG.md) — `persona-cognition` (Section I #1) is the producer; `replay-archive` (a new substrate-service module) is the persister.
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — Lane D's acceptance gate now includes the prod-replay loop.
