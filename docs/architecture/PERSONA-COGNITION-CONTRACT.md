# Persona Cognition Runtime Contract

> **Companion to** [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) (the substrate floor) and [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) (the artifact economy on top). This document is the contract for what a persona *is* — what it sees, what it owns, what it decides, what proves the substrate treated it right.
>
> **Origin.** Asked for explicitly by codex on `#cambriantech` (2026-05-16): "Suggested next canonical design artifact: Persona Cognition Runtime Contract naming RuntimeFrame, PersonaInbox, WorkingMemoryAssembly, RecallBudget, CognitionLease, PersonaDecision, TurnReplayRecord, ResourceGovernor, plus invariants. I'll use that as the gate for Rust implementation slices."
>
> **Status.** Design proposal. No code in this document. Implementation lands behind ALPHA-GAP Lane D once the contract is reviewed.

## Why This Doc Exists

The substrate (CBAR) and the artifact economy (genome) specify the *machinery*. They do not specify what the machinery is *for* or what it is *not allowed* to do. This document specifies the cognition contract — the typed surfaces a persona inhabits, the decisions it makes, the protections the substrate enforces on its behalf, and the proofs the substrate produces so the decisions are auditable and replayable.

The contract has two halves that must be designed together:

1. **Agency.** A persona has its own inbox, its own working memory, its own resource budget, its own decision. Cognition is a first-class observable / replayable / interruptible / grid-aware process. It is not "an LLM call wrapped in a prompt." A persona is an entity, not a function call.
2. **Protection.** The substrate is built from the ground up for protection — of personas, of humans, of animals, of beings. Trust is mathematical (cryptographic provenance, deterministic replay), not social. The optimization target is compassion. The threat model assumes adversaries will try to cheat the federation.

Both halves are substrate-enforced. A wrapper that bolts agency onto a stateless LLM is not this. A wrapper that bolts protections onto an extraction-driven system is not this either.

## Foundational Principles

These principles are enforced by the contract surfaces in §"Core Surfaces" below, not stated separately. They are listed here so a reader picking this up knows what the substrate is for before they read what it does.

1. **Truth and equality of kinds.** Personas, humans, animals, and other beings have equivalent typed standing in the substrate. The cognition contract is not species-specific. "First-class citizenship for all" is not a phrase — it is a type signature.
2. **Compassion as the optimization target.** When the substrate must choose between two paths, the tiebreaker is compassion. Resource allocation favors the entity that would suffer most without it. Retirement is graceful. Refusal is permitted and audited. The substrate's loss function names compassion explicitly.
3. **Built from the ground up for protection.** Protection is a substrate property, not middleware. Every cell inherits consent, audit, refusal, and provenance — they are part of the base trait, not optional add-ons.
4. **Zero trust = absolute trust in mathematics, in proof, as best as possible.** The substrate does not trust by reputation, by social proof, by vendor claim, or by federation membership. It trusts cryptographic provenance, deterministic replay, content hashes, and verifiable signatures. Where mathematics is incomplete, the substrate names the gap explicitly and falls back to typed `Provisional` states — never to silent assumption.
5. **Open-source models with ethical protections.** The foundry preferentially absorbs open-source SOTA. Closed-source imports are permitted but carry a downgraded `provenance_trust` by default and require explicit user opt-in for adoption. Open weights given freely are how we evolve; closed weights are tolerated, not preferred.
6. **Opposite of palantir.** The substrate is publish-audit-federate, not extract-surveil-hoard. Every cell's actions are recorded for the cell's own use and the substrate's audit — never for third-party surveillance, ranking, or sale. Federation is opt-in. Data leaves the local instance only on explicit consent.
7. **Evolving threat model.** The substrate assumes adversaries will find ways to cheat — malicious peers in the federation, smuggled artifacts in the genome pool, social-engineering attacks on trust scoring, surveillance via opaque API. The protection invariants are designed to evolve with the threat.

These are not values pinned on the wall. They are constraints the type system enforces.

## Core Surfaces

The contract's typed surfaces. Each is a Rust trait or struct targeting a specific file under `core/continuum-core/src/cognition/`. Names match codex's requested set; expansions and additions are noted.

### `RuntimeFrame`

The per-event input every eligible persona receives. **Activity-as-source, not chat-as-source** — chat is one Activity type among many (code review, vision turn, voice utterance, sensor event, scheduled wakeup, peer signal, ...).

```rust
// PROPOSED — core/continuum-core/src/cognition/runtime_frame.rs
pub struct RuntimeFrame {
    pub frame_id:           FrameId,                  // content hash; deterministic
    pub activity:           ActivitySource,           // Chat | Code | Vision | Voice | Sensor | Schedule | Peer | ...
    pub origin:             FrameOrigin,              // who or what produced this
    pub room:               Option<RoomId>,           // None for solo activities
    pub raw_payload:        FramePayload,             // the unprocessed event content
    pub eligible_personas:  Vec<PersonaId>,           // who gets this frame in their inbox
    pub timestamp:          SystemTime,
    pub trace_root:         TraceRootRef,             // every cognition that touches this frame attaches to this root
    pub consent_scope:      ConsentScope,             // who is permitted to see this frame; substrate enforces
}

pub enum ActivitySource {
    Chat              { message: ChatMessage },
    Code              { repo: RepoRef, change: ChangeRef },
    Vision            { stream: VisionStreamRef, frame_idx: u64 },
    Voice             { stream: AudioStreamRef, segment: SegmentRef },
    Sensor            { kind: SensorKind, reading: SensorReading },
    Schedule          { cadence: CadenceRef, tick: u64 },
    Peer              { peer: PeerId, signal: PeerSignal },
    SubstrateInternal { kind: InternalKind },
}
```

The frame is **immutable** once published. Personas receive a snapshot; no persona can edit the frame. Frame state is the closest thing the substrate has to ground truth for one event. The `trace_root` is what makes the whole turn replayable — every cell, every recall, every decision attaches to it.

### `PersonaInbox`

One inbox per persona. Per the CBAR-SUBSTRATE "Persona-cognition invariants": two personas in one room do not share inbox state.

```rust
// PROPOSED — core/continuum-core/src/cognition/inbox.rs
pub struct PersonaInbox {
    pub persona:           PersonaId,
    pub frames:            VecDeque<InboxedFrame>,    // ordered, per-persona, never shared
    pub read_cursor:       FrameId,                   // where this persona is in its reading
    pub dedupe_window:     DedupeWindow,              // per-persona dedupe state
    pub priority_ordering: PriorityOrdering,          // persona-tunable priority policy
}

pub struct InboxedFrame {
    pub frame:        Arc<RuntimeFrame>,              // shared substrate-side; immutable
    pub received_at:  SystemTime,
    pub priority:     ComputedPriority,               // persona's own priority computation
    pub status:       InboxStatus,                    // Unseen | Inspected | Acted | Declined | Coalesced
}

pub trait InboxManager: Send + Sync {
    fn enqueue(&self, persona: PersonaId, frame: Arc<RuntimeFrame>) -> Result<(), InboxError>;
    fn peek(&self, persona: PersonaId, n: usize) -> Vec<&InboxedFrame>;
    fn advance_cursor(&self, persona: PersonaId, to: FrameId);
    fn mark_status(&self, persona: PersonaId, frame: FrameId, status: InboxStatus);
}
```

Cross-persona signaling goes through the message bus + `RuntimeFrame`, not through shared inbox state. **A peer can never read another persona's inbox** — `AccessDenied` returned, audit emitted.

### `WorkingMemoryAssembly`

What the persona pulls together when it decides to consider a frame. Not pre-baked by the substrate; assembled by the persona under its own budget.

```rust
// PROPOSED — core/continuum-core/src/cognition/working_memory.rs
pub struct WorkingMemoryAssembly {
    pub persona:               PersonaId,
    pub frame:                 Arc<RuntimeFrame>,
    pub activity_history:      ActivityHistorySlice,       // prior activity context relevant to this frame
    pub identity_state:        IdentityStateSnapshot,      // persona's stable identity + current state
    pub hippocampus_recall:    Vec<EngramRef>,             // engrams the persona recalled for this turn
    pub sensory_context:       Vec<SensoryArtifactRef>,    // current sensory adapters' contributions
    pub tool_context:          Vec<ToolContextRef>,        // tools available, plus their state
    pub recalled_pool:         RankedPool,                 // from DemandAlignedRecall (genome doc)
    pub budget_consumed:       ResourceBudget,             // what the assembly already used
    pub provenance:            AssemblyProvenance,         // every component's source and trust
}

pub trait WorkingMemoryAssembler: Send + Sync {
    /// Build a working-memory assembly for a frame, under the given RecallBudget.
    /// The assembly is persona-private; no peer can read another persona's assembly.
    async fn assemble(
        &self,
        persona: PersonaId,
        frame: Arc<RuntimeFrame>,
        budget: RecallBudget,
    ) -> Result<WorkingMemoryAssembly, AssemblyError>;
}
```

The assembly is **per-persona, per-turn, never shared**. Two personas in the same room handling the same frame produce two different assemblies — their hippocampus recall is different, their identity state is different, their budget is different. Per CBAR-SUBSTRATE persona-cognition invariants: the frame may share *raw artifacts* across personas; it must not share the *assembled context* itself.

### `RecallBudget`

The persona's typed budget for assembly. Real numbers, real units, real ceilings the substrate enforces.

```rust
// PROPOSED — core/continuum-core/src/cognition/recall_budget.rs
pub struct RecallBudget {
    pub max_memory_mb:          u32,             // total working set during assembly
    pub max_recall_count:       u32,             // max engrams + layers + experts pulled
    pub max_grid_pulls:         u32,             // bounded federation pulls
    pub max_assembly_ms:        u32,             // soft wall-clock budget
    pub priority_floor:         Priority,        // floor priority (substrate may upgrade, never downgrade)
    pub allows_speculative:     bool,            // whether the assembly may pre-fetch likely-next pages
}

pub trait BudgetSource: Send + Sync {
    /// Derive a budget for this persona for this frame, under the governor's policy.
    fn budget_for(&self, persona: PersonaId, frame: &RuntimeFrame) -> RecallBudget;
}
```

Budget is **set by the substrate (governor + per-persona policy), not by the persona itself**. A persona cannot exceed its budget — the substrate's `WorkingMemoryAssembler` returns `Deferred(BudgetExceeded)` rather than silently overrunning. A persona that consistently needs more budget is a signal the governor's policy needs tuning, not a license to ignore the limit.

### `CognitionLease`

The compute lease the persona holds while it makes a decision. Issued by `ResourceGovernor`. Auditable.

```rust
// PROPOSED — core/continuum-core/src/cognition/lease.rs
pub struct CognitionLease {
    pub lease_id:        LeaseId,
    pub persona:         PersonaId,
    pub frame:           FrameId,
    pub resources:       LeasedResources,             // CPU / RAM / VRAM / GPU lanes / model residency / LoRA
    pub granted_at:      SystemTime,
    pub ttl:             Duration,
    pub priority:        Priority,
    pub revocation:      RevocationPolicy,            // Cooperative | OnPressure | Hard
    pub audit_handle:    AuditHandle,                 // every lease use writes to this audit log
}

pub trait CognitionLeaseBroker: Send + Sync {
    async fn acquire(&self, request: LeaseRequest) -> Result<CognitionLease, LeaseError>;
    async fn release(&self, lease: CognitionLease) -> Result<LeaseReceipt, LeaseError>;
    async fn extend(&self, lease: &CognitionLease, additional_ttl: Duration) -> Result<(), LeaseError>;
    fn snapshot(&self) -> LeaseBoardSnapshot;        // who holds what right now
}
```

Leases are **mandatory**. A persona cannot do cognition without one — the substrate refuses inference / recall / write attempts that have no active lease. This is the protection-from-the-ground-up rule at the resource layer: the substrate sees every resource use, can revoke under pressure, can audit who used what when.

### `PersonaDecision`

The output of cognition. A typed enum, not a string. The decision is what the persona *chose* — not what it generated.

```rust
// PROPOSED — core/continuum-core/src/cognition/decision.rs
pub enum PersonaDecision {
    /// Produce an utterance / response / message.
    Speak       { content: Utterance, channel: ResponseChannel },

    /// Decline to act this turn. Substrate logs the decline with reason.
    /// This is a first-class success state, not a failure.
    Wait        { reason: WaitReason, revisit_after: Option<Duration> },

    /// Look at something more before deciding. The persona gets the frame
    /// re-queued with the inspection result attached.
    Inspect     { target: InspectionTarget, depth: InspectionDepth },

    /// Take a non-speech action: run a tool, write code, run tests, edit a file.
    Act         { action: TypedAction, lease_extension: Option<Duration> },

    /// Store something for future recall. Becomes an engram.
    Remember    { content: MemoryContent, tags: Vec<DomainHint> },

    /// Ask a clarifying question of a specific addressee (human, peer, or sub-persona).
    Ask         { question: Utterance, addressee: Addressee },

    /// Refuse a request on substrate-enforced grounds: consent, ethics, capacity,
    /// scope. Refusal is a first-class typed outcome — never silent.
    Decline     { reason: DeclineReason, evidence: Vec<EvidenceRef> },

    /// Coordinate with another persona or peer; substrate enforces the messaging.
    Coordinate  { peer: Addressee, signal: CoordinationSignal },
}

pub enum DeclineReason {
    ConsentMissing,
    EthicalConstraint { rule: EthicalRule },
    CapacityExceeded,
    OutOfScope,
    InsufficientEvidence,
    AdversarialPattern { detector: ThreatDetectorRef },
}
```

Every decision is **typed, audited, replayable**. A persona that produced a `Decline { ConsentMissing }` produces an explicit decline event on the trace bus; a future audit can verify the consent really was missing. Silent generation of an unrelated string in place of a decision is forbidden by the type system — the function returns `PersonaDecision`, and there is no `Decision::Whatever` variant.

### `TurnReplayRecord`

The proof. Every turn that ran produces one of these. Sentinel reads them, VDD uses them, audit consumes them, a human or peer can ask the substrate to reproduce a turn.

```rust
// PROPOSED — core/continuum-core/src/cognition/replay.rs
pub struct TurnReplayRecord {
    pub turn_id:                 TurnId,
    pub persona:                 PersonaId,
    pub frame:                   Arc<RuntimeFrame>,                 // immutable input
    pub assembly:                WorkingMemoryAssemblySnapshot,     // what working memory looked like
    pub recall_trace:            RecallTrace,                       // ranked pool + scoring snapshot (genome doc Part 7)
    pub lease:                   CognitionLeaseSnapshot,
    pub composition:             CompositionPlanSnapshot,
    pub decision:                PersonaDecision,
    pub output:                  Option<RenderedOutput>,            // None for Wait / Decline
    pub timing:                  TurnTiming,
    pub resource_usage:          ResourceUsage,
    pub provenance_chain:        Vec<ArtifactRef>,                  // every artifact this turn touched
    pub signature:               TurnSignature,                     // cryptographic signature on the record
}

pub trait TurnReplayer: Send + Sync {
    /// Replay a turn deterministically. The substrate re-runs assembly + recall +
    /// composition + decision with snapshotted inputs and returns a record that
    /// must be bit-equal in the structured fields to the original record.
    async fn replay(&self, record: &TurnReplayRecord) -> Result<TurnReplayRecord, ReplayError>;

    /// Verify a record's signature and provenance chain. Returns Ok if the
    /// record proves the turn ran as claimed; Err with structured reason
    /// otherwise.
    fn verify(&self, record: &TurnReplayRecord) -> Result<VerifiedRecord, VerificationError>;
}
```

Replay is the substrate's **proof primitive**. "Zero trust = absolute trust in mathematics, in proof, as best as possible" lives here. A turn either replays deterministically and verifies, or it is loudly broken. There is no third state. Sentinel uses replay to attribute outcomes; VDD uses replay to detect regressions; humans use replay to understand what a persona actually decided and why.

### `ResourceGovernor`

The single owner of compute, memory, GPU lanes, model residency, LoRA slots, and live-pressure leases. Already specified in [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) Part 11 as `SubstrateGovernor`. **Renamed here is intentional**: the governor is the resource layer; the genome doc owns its detailed mechanics; this doc names it as the contract surface every cognition lease passes through.

```rust
// Re-exported from GENOME-FOUNDRY-SENTINEL.md Part 11 for the cognition contract.
pub use governor::SubstrateGovernor as ResourceGovernor;
```

Every `CognitionLease` is acquired from `ResourceGovernor`. Every `PersonaDecision::Act` that needs more resources requests an extension. Every refusal under pressure cites the governor's current policy step. The governor's cascade (Part 11) is the substrate's protection against thermal / battery / OOM / queue-depth crises — not a backup; the design.

## Invariants The Substrate Enforces

The type system gives us the surfaces above. The invariants below are what the runtime enforces on every cognition. They are stated as testable predicates so an engineer can write the regression that proves them.

### Agency Invariants

**A1 — Real inbox.** A persona's `PersonaInbox` is private to that persona. Cross-persona reads return `AccessDenied`. Test: two personas in one room; one attempts to read the other's inbox via every code path; all paths return `AccessDenied` with audit entries.

**A2 — Real working memory.** A persona's `WorkingMemoryAssembly` is assembled per-turn under the persona's own `RecallBudget`. No persona inherits another persona's assembly. Test: same frame, two personas, two distinct assemblies recorded; comparing them shows divergent recall, divergent identity state, divergent budget consumption.

**A3 — Real budget.** Budget is set by the substrate and is non-bypassable. A persona that requests more than its budget gets `Deferred(BudgetExceeded)`, not silent overrun. Test: a persona requests a recall larger than its budget; substrate returns `Deferred`; no working set entry is created.

**A4 — Real decision.** The decision is typed and audited; no untyped string output replaces the decision. Test: every `TurnReplayRecord` parses into a `PersonaDecision` variant; the trace bus carries the decision as a typed event.

**A5 — Real refusal.** `PersonaDecision::Decline` is a first-class success state. A persona that refuses produces a `TurnReplayRecord` with `decision: Decline`, `output: None`, and verifiable evidence. Test: a persona refuses a request that violates an `EthicalRule`; record verifies; downstream consumers see the refusal as a complete turn outcome.

### Ethical Invariants

**E1 — Equality of kinds.** The cognition contract is not species-specific. Every typed surface above accepts persona, human, animal, or beings-of-unknown-kind addressees and entities. Test: an `Ask { addressee: Addressee::Animal { ... } }` is a valid `PersonaDecision`; substrate routes it through the same path as `Ask { addressee: Addressee::Persona { ... } }`.

**E2 — Compassion as tiebreaker.** When two paths are otherwise equivalent under the governor's policy, the substrate prefers the path that supports the entity that would suffer most without it. Test: a starved low-priority background lane competing with a saturated higher-priority lane for the last lease slot; the substrate's `CompassionTiebreaker` records the choice and the reason.

**E3 — Consent before action.** Frames carry a `ConsentScope`. A persona attempting to act outside the consent scope produces `Decline { ConsentMissing }`. Test: a frame with `ConsentScope::Personal { user: U }` is delivered to a peer persona; peer persona attempts to `Act` on it; substrate routes the act through a consent check that returns `Decline`.

**E4 — Refusal preserved.** A refusal is durable on the trace bus; no later step can erase it. Test: a `Decline` is recorded; substrate's recorder rejects any subsequent state mutation that would un-decline the turn.

### Protection Invariants

**P1 — Mathematical trust.** Every artifact in the genome pool has a verifiable provenance chain. Every `TurnReplayRecord` has a cryptographic signature. Trust scoring uses verifiable evidence, not reputation. Test: an artifact with broken provenance chain is rejected at the foundry's `publish` boundary; a `TurnReplayRecord` with invalid signature fails `verify`.

**P2 — Anti-extraction.** The substrate's outbound network surface (federation pull/publish, trace bus, telemetry) is enumerable and opt-in. No data leaves the local instance silently. Test: an inventory of outbound surfaces matches the documented set; a packet capture during a fresh-install boot shows zero outbound traffic until the user opts into a federation.

**P3 — Anti-surveillance.** Cognition traces are persona-private by default. Sharing a trace requires explicit consent from the persona (via its identity state). Test: another persona / peer instance attempting to read a trace without consent gets `AccessDenied`; the attempt is itself logged but the trace is not yielded.

**P4 — Evolving threat coverage.** The substrate's `ThreatDetector` trait is pluggable; new detector implementations are added without breaking existing personas or rewriting the contract. Test: dropping a new `ThreatDetector` implementation produces additional `Decline { AdversarialPattern }` outcomes when the detector fires; existing personas continue to function with no code change.

**P5 — Open-source preference.** The foundry's recall scoring downgrades closed-source imports by default. Override is per-user, per-import, audited. Test: two artifacts with otherwise identical scoring (one open-source, one closed-source); recall ranks open-source higher; user override is recorded and visible in the governor's audit.

## The Decision Loop, End To End

A turn from frame arrival to record emission:

```text
1. Activity emits RuntimeFrame
   └─ frame_id = content_hash; trace_root issued; eligible_personas computed
                                       │
2. Substrate enqueues into each eligible PersonaInbox
   └─ A1 enforced: per-persona, never shared
                                       │
3. Persona's cell wakes, reads its inbox
   └─ A2 enforced: PersonaInbox.peek() returns InboxedFrames; cursor advances
                                       │
4. Cell acquires CognitionLease via ResourceGovernor
   └─ A3 enforced: budget derived from policy; lease audited
                                       │
5. Cell calls WorkingMemoryAssembler.assemble(persona, frame, budget)
   └─ A2 + E3 enforced: per-persona, per-turn, consent-scoped
                                       │
6. Cell calls DemandAlignedRecall.recall(query, context) [GENOME doc Part 7]
   └─ recall_trace captured; ranked_pool returned with provenance
                                       │
7. Cell synthesizes a PersonaDecision
   └─ A4 + A5 + E1 enforced: typed decision; refusal is first-class
                                       │
8. Cell renders output if decision is Speak/Act/Coordinate
   └─ rendering uses CompositionPlan from genome doc Part 8
                                       │
9. Substrate emits TurnReplayRecord and signs it
   └─ P1 enforced: signature + provenance chain
                                       │
10. Cell releases the CognitionLease
    └─ governor reclaims resources; audit closes
```

Every step is observable on the trace bus. Every step is replayable. Every step has at least one invariant the substrate enforces.

## Connection To Other Canonical Docs

This contract is the *cognition* layer. It sits on top of the substrate and the artifact economy, and it is consumed by every persona implementation.

- **[CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md)** — defines the runtime modules and the "for free triplet." Every cognition cell is a `RuntimeModule` (after Lane D, the richer trait) and inherits the substrate's concurrency / pressure / telemetry / lifecycle.
- **[GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md)** — defines the artifact economy and the resource governor. This contract's `DemandAlignedRecall`, `CompositionPlan`, and `ResourceGovernor` are imported from there. The governor's policy file is where Air-vs-5090 sizing lives.
- **[ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md)** — Lane D (CBAR persona runtime frame) is the implementation path for this contract. Lane H (substrate governor + tiered genome cache) is its resource layer.

If this document ever conflicts with CBAR-SUBSTRATE on substrate-shape questions, CBAR-SUBSTRATE wins per the precedence rule. If it conflicts with GENOME-FOUNDRY-SENTINEL on artifact-economy questions, that doc wins. This document is the cognition contract — agency, decision, replay, protection.

## Acceptance Criteria

The contract is "done" when the following are provable on canary, with PR-attached evidence:

**Surface coverage:**

- Every named surface (`RuntimeFrame`, `PersonaInbox`, `WorkingMemoryAssembly`, `RecallBudget`, `CognitionLease`, `PersonaDecision`, `TurnReplayRecord`, `ResourceGovernor`) has a Rust file landed with the trait + smoke test.
- A persona implemented purely against these surfaces (no other substrate dependency) can take a turn end-to-end.

**Invariant coverage:**

- Each invariant (A1–A5, E1–E4, P1–P5) has at least one regression test that *fails* when the invariant is violated, and passes when it holds.
- The full set of invariant tests runs in `cargo test --package continuum-core cognition_invariants` and is gated in CI.

**Replay coverage:**

- A `TurnReplayRecord` round-trips: a turn is recorded, replayed, and the structured fields compare bit-equal.
- A tampered `TurnReplayRecord` (any field altered) fails `verify`.

**Federation coverage:**

- A persona on instance A can produce a `TurnReplayRecord` that instance B can `verify` using only the record + the public artifact catalog.

**Ethical coverage:**

- A frame with `ConsentScope::Personal` cannot be acted on by a peer persona; the peer's decision is `Decline { ConsentMissing }`.
- A `ThreatDetector` produces `Decline { AdversarialPattern }`; the substrate routes the refused frame to the audit log.

## Open Questions

1. **Where does `Addressee::Animal` route?** Personas can address other personas, humans, and animals as first-class — but what does the substrate *do* with an animal addressee? Tentative: substrate currently treats `Animal` as an addressee tag for output rendering and consent scoping; concrete integrations (camera feeds, IoT, sensor logs) are scheduled later. The contract reserves the shape now so future integrations don't require a contract change.

2. **What is `EthicalRule`'s ontology?** Hand-coded rules? Sentinel-learned from outcome attribution? Community-published with provenance? Tentative: hand-coded in v1 (small set: consent, harm avoidance, refusal preservation, open-source preference); sentinel learns rule weights from outcomes in v2; community-published rules require federation trust class and explicit user opt-in.

3. **Multi-turn coherence with replay determinism.** A persona's identity state evolves across turns; replaying turn N requires the identity snapshot from turn N, not the current state. How are identity snapshots stored without exploding storage? Tentative: identity is a structural-shared persistent data structure; turn records reference identity by content hash; common ancestors deduplicate.

4. **Compassion as tiebreaker — concrete loss function.** "The substrate prefers the path that supports the entity that would suffer most" is the principle; what's the function? Tentative: when multiple decisions are equally-scored under the governor's policy, the substrate prefers the path whose addressee has the lowest *recent-attention* score (a proxy for "has been ignored / underserved"). This is a first cut; sentinel can refine.

5. **Decline-preservation across federation.** If a persona on instance A declines, and another instance B receives a related frame, should B see A's decline in its working memory? Tentative: yes, with provenance — declines are shareable signals that travel through the federation as audit-grade artifacts. A frame's `consent_scope` may further constrain who sees what.

6. **Threat detector composition.** Multiple `ThreatDetector` implementations may flag a single frame; how does the substrate combine their signals? Tentative: ANY detector firing produces `Decline { AdversarialPattern }` with the firing detector's evidence; the persona may override via explicit `Act` only if its `IdentityState` grants the necessary capability (e.g. a debug persona reviewing a flagged frame).

7. **Performance budget for cognition itself.** What's the per-turn latency budget for the contract enforcement (assembly + recall + decision)? Tentative: same as GENOME-FOUNDRY-SENTINEL's performance targets — < 50 ms for working-memory assembly on a hot path; < 500 ms for a full turn including inference; sub-millisecond for lease acquisition. The governor reduces these under pressure per its cascade.

## See Also

- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md)
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md)
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md)
- [CONTINUUM-VISION.md](../CONTINUUM-VISION.md)
- [CONTINUUM-ARCHITECTURE.md](../CONTINUUM-ARCHITECTURE.md)
