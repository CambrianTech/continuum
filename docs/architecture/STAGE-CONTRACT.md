# The stage contract (M0), version 1

Status: DESIGN FOR REVIEW (2026-11-01), revision 2 with Codex's boundary review at eeff13c3. Not
frozen until Codex and Cormac pass it. One versioned contract before parallel implementations
(Codex's coordination, airc 1c9c2644). Owners: Fable (this contract; perception and reflex
migration), Cormac (the dream and learning consumer; chunk yield and wake semantics), Codex
(shared task lifetime and terminal delivery; admission and the physical grant). Each owner
reviews the other two boundaries. THE-WHOLE-MIND §1b is the why; this is the what.

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [1. Types (owned by `continuum-kernel/src/runtime/stage/`)](#1-types-owned-by-continuum-kernelsrcruntimestage)
- [2. Contracts at the three boundaries](#2-contracts-at-the-three-boundaries)
- [3. First migrations (Fable)](#3-first-migrations-fable)
- [4. The shared validator (all three owners)](#4-the-shared-validator-all-three-owners)

## 1. Types (owned by `continuum-kernel/src/runtime/stage/`)

```rust
/// A brain function's identity, stable across restarts.
pub struct StageId(Uuid);
/// One running instance of a stage for one mind. Allocated atomically by the stage owner; a new
/// incarnation (restart, migration, re-host) adopts the previous one's open obligations with a
/// durable receipt (below) and fences its mutations.
pub struct StageIncarnation { pub stage: StageId, pub mind: MindId, pub epoch: u64 }
/// One run within an incarnation: what a dispatch, a control receipt and an output are tied to.
pub struct RunId { pub incarnation: StageIncarnation, pub seq: u64 }

/// A routing identifier AND its payload contract: the typed payload a topic carries, and whether
/// it is a replayable event stream (every item matters, read through a cursor) or a replaceable
/// snapshot (only the latest matters, read by revision).
pub struct InputTopic<P> { pub name: &'static str, pub kind: TopicKind, _payload: PhantomData<P> }
pub enum TopicKind { ReplayableEvents, ReplaceableSnapshot }
/// The exact input a run saw: topic and the revision or cursor it read through, captured at
/// dispatch.
pub struct InputRevision { pub topic: &'static str, pub through: u64 }

/// What a stage produces, with provenance: which run, from which input revisions.
pub struct Provenance { pub by: RunId, pub from: Vec<InputRevision> }

/// When a stage runs. Triggers COMPOSE: a stage declares a set (an input moved, a dispatched
/// task completed, a salience threshold crossed, and the never-comatose cadence floor together).
pub struct Triggers { pub inputs: Vec<&'static str>, pub completions: bool, pub salience: Option<Salience>, pub floor: Option<Duration> }

/// What a run costs. Execution admission and retained-state residency are SEPARATE claims on the
/// one physical resource authority (`resources::daemon`, `LeaseRequest` -> `LeaseGuard`):
/// executing (CPU time, or a model call at a tier) and keeping state resident (paused KV,
/// weights, buffers: bytes that stay charged while paused or yielded). CPU work has a cost and
/// bounded outstanding demand too; recall's embedding and reranking are not free.
pub struct Cost { pub execution: ExecutionCost, pub residency: ResidencyCost }
pub enum ExecutionCost {
    Cpu { estimated: Duration },
    Model { tier: ModelTier },
    /// A share of one lane's engine time under a learn grant, debited against the engine-side
    /// ledger shared with /train (grant id, engine incarnation, period, share_ppm), so the authority
    /// sums learning across minds without guessing it from tiers (Cormac on #5692).
    Learn { share_ppm: u32, period: Duration },
}
```

The stage context carries a typed **authorized capability** for its admitted execution and
residency, owned by the resource authority, never a cloneable or borrowed guard whose lifetime
ends silently when `run` returns.

A stage implements:

```rust
#[async_trait]
pub trait Stage: Send + Sync + 'static {
    fn id(&self) -> StageId;
    fn triggers(&self) -> Triggers;
    fn cost(&self, input: &[InputRevision]) -> Cost;
    /// One run. Long work is a task (the shared task owner) whose completion re-triggers.
    async fn run(&self, ctx: &StageContext, input: &[InputRevision]) -> RunOutcome;
}

pub enum RunOutcome {
    /// outputs to publish under the publication rule (below), each with Provenance
    Produced(Vec<Output>),
    /// dispatched work as a task, with the RunId and input revisions captured at dispatch
    Dispatched(TaskRef),
    /// nothing to do for these revisions
    Idle,
    /// stopped at a yield point; the receipt proves which stop happened (below)
    Stopped(StopReceipt),
}
```

### Control and its receipts

A control request (pause, yield, cancel) is a request; only a receipt proves the stop. Receipts
and suspended-execution handles are owned by the task and runtime owner (Codex), across the
caller returning, a forgotten handle, and restart.

- **Pause:** a quiescence receipt plus a retained continuation; execution and residency custody
  both remain charged. A stage or engine that cannot pause returns a typed refusal, never a
  dropped future presented as paused.
- **Yield:** a continuation plus a receipt that execution capacity was released; residency is
  named: which state stays charged, or where it was paged and under whose custody.
- **Cancel:** the effects already committed are recorded, and the run becomes terminal only
  after quiescence.

### Publication

Within one incarnation, overlapping runs publish with a compare-and-set on the input revisions
they derived from: an older run's result never overwrites state derived from newer input.

### Dataflow: artifacts and dependent stages (2026-11-14)

A stage emits what it delivers; whatever needs it subscribes (CONTINUUM-ENGINE.md rule 10). The
emission is owned once in `runtime/stage/artifact.rs`, so no feature hand-wires a `oneshot`, a
`Notify` or a flag for it:

- `artifact::<T, F>(topic)` opens a produced-once artifact: the producer holds `Produces`
  (`produce(T)` or `fail(F)`), any number of consumers hold a cloneable `Subscribes` and receive
  `Arc<T>` (a handle, never a copy). `handoff::<T, F>(topic)` is the one-consumer form that MOVES
  the value (an admission, a guard). Every artifact settles exactly once: `Ready`, `Failed(F)`
  typed by its stage, or `Abandoned`, which a dropped producer raises (a task that ended, panicked
  or was cancelled), so a waiter never waits on a producer that is gone.
- A consumer's wait returns `Awaited { settled, arrival }`: `AlreadySettled` (the producer's work
  overlapped completely) or `WaitedFor(duration)`. The settle and the wait are probed for free
  (`stage.artifact.settled`, `stage.artifact.awaited`).
- `DependentStage` (`runtime/stage/dependent.rs`) is a pass of the act graph: it names the
  artifact it produces, declares its inputs as a tuple of `Subscribes`, and runs ONCE, consuming
  itself, when the LAST input settles. `spawn_when_ready(stage, inputs)` gives it its own task, the
  wake, the inputs by handle, its output as an artifact (abandoned if the run panics) and its
  timing (`stage.dependent.ran`). What a missing input means is the stage's to decide, from its
  typed `Settled`.

First consumers: prefill during the tool (outlier A: `cognition/llm_deliberation_faculty/
prefill_during_tool.rs`, a `DependentStage` whose artifact her next generation subscribes to) and
the endpoint-retained page IO's result (outlier B: `inference/turn_admission/paging_lifetime.rs`,
a handoff of the turn's admission that replaced its `oneshot`). Follow-up, owner Fable: the
`#[derive]` half of the for-free triplet, once a third consumer shows what it should generate.

## 2. Contracts at the three boundaries

- **Task lifetime and delivery (Codex).** A dispatched run's completion goes through the shared
  task owner: durable before delivered, delivered at least once, accepted idempotently by task
  identity. **Fencing blocks stale mutation, not durable evidence:** a completion from an old
  incarnation stays in the task ledger, and the current incarnation explicitly adopts, consumes
  or disposes of it with a durable receipt. Nothing strands a completion or erases a committed
  effect. Dependencies, not available guarantees: #5585 (unmerged; it superseded #5664) do not yet prove
  restart adoption or cancellation quiescence; Codex owns closing those gaps.
- **Admission and the physical grant (Codex).** One authority grants physical capacity,
  execution and residency separately. The stage scheduler ranks ready demand by each mind's
  state (arousal, engagement, age; THE-WHOLE-MIND §4.2), holds no FIFO in front of a mind, and
  never grants more than the authority admitted.
- **The dream consumer (Cormac).** Dreaming is a stage with a `Learn` execution cost and a
  residency cost (its shadow adapter and optimizer moments). Its inputs are the settled-experience
  topic (replayable) and her dream-share setpoint (replaceable snapshot). Amended from Cormac's
  measurements (#5692 review, fork 832d78cb1):
  - **The chunk is one engine learn step, sized by measurement.** One `/learn` step is a single
    non-preemptible forward+backward graph: 962 ms median on the IntelMac CPU with Qwen2.5-0.5B,
    and a streaming decode's max inter-token gap rose from 288 ms to 991 ms beside it (p50/p95
    unchanged). Pause and yield land at the next step boundary, so the wake latency is one step,
    stated per node class as measured (IntelMac CPU ~1 s; M5/Metal pending, Fable). A smaller bound
    needs the step's graph split across layers, never token chunking.
  - **Every dream step is `Dispatched` work** through the task owner (Codex), bound before any
    effect to the stage `RunId` (provenance), the task owner's `TaskExecution` (custody), its input
    revisions and the admitted grant. The completion
    carries `{ step, loss }` (fork #86 adds the committed step) and re-triggers the stage. The
    #5693 runner refuses `Dispatched` until the task owner's registration exists; the dream stage
    lands after it.
  - **Receipts:** `Cancelled.committed_effects` = optimizer steps committed by this run;
    `Yielded.residency_kept` = 0 when the stage snapshots its shadow and releases the optimizer
    moments (fork #86's snapshot), else the shadow + moments bytes; `Paused` keeps both. A pause or
    cancel requested mid-step gets its receipt only when that step's outcome arrives (quiescence).
    If the engine's incarnation changes mid-step, that outcome never arrives, so quiescence is
    proved by the fence, never by waiting: fork #88 stamps each learn with `{ execution, base_step,
    engine_incarnation }`, a step under an old incarnation can no longer commit once its physical executor is proven stopped (a new incarnation id alone is not that proof), and
    `committed_effects` comes from `/learn/receipt` for that execution, else from the adapter's
    `base_step` against the step the run dispatched, under #5695's lost-reply order (task terminal,
    then the same-incarnation receipt, then redispatch, then rewind to the binding snapshot, else
    unresolved).
  - **The input ack is the committed step, not admission:** the settled-experience topic is
    acknowledged through cursor N only when the step that consumed N reports its step committed,
    and the snapshot's `Provenance.from` is the cursor range it consumed (commit-once,
    ONE-RESIDENT-MODEL §9.3).

## 3. First migrations (Fable)

- **Recall (outlier A):** today `recall_faculty` wrapped in `deferred_faculty`. It becomes a stage
  whose input is her perception revision, whose execution cost is measured CPU (embedding and
  reranking included, not free), and whose output is a recall track with provenance.
- **Reflex (outlier B):** today `directed_pending` plus `TurnAttention::for_message` at four
  sites. It becomes a stage whose input is the perception topic (from the store, PERCEPTION-
  FROM-THE-STORE.md), whose execution cost is measured CPU, and whose output is the addressed trip
  (`PersonaIdentity::addressed_by`, species-blind).

Deleted when their migrations land: the `BrainRegion::tick` cadence-only interface (regions
move to `Stage` with a `Floor` trigger), `deferred_faculty`'s wrapper, `directed_pending`'s
global map, the four per-site attention derivations.

## 4. The shared validator (all three owners)

Recall, reflex and dream under contention on one replayed engine, each case followed by useful
work (the mind keeps working, both ways: law 8):
- a stimulus during work: the reflex trips; the dream yields at its next step boundary, the receipt
  carrying the measured expected latency for the node class beside the observed one;
- the engine relaunches mid-step, two cases: (a) the fence's evidence is present (`/learn/receipt`
  for the execution, or `base_step` read from the adapter): the cancel receipt carries the proven
  committed count and nothing re-applies the step; (b) that evidence is missing: the run stays
  `Unresolved` with its custody retained, never a cancel receipt or a guessed count. A new
  incarnation id alone does not prove the old executor stopped;
- late and duplicate completions; a late terminal delivery after a restart, adopted with a receipt;
- a stale incarnation's mutation refused while its completion stays adoptable;
- a forgotten continuation (nobody resumes a paused run): its custody is reclaimed, reported;
- a stale run's output refused by the publication rule;
- an unsupported pause: a typed refusal, never a silent drop;
- a restart in the middle of each.

## 5. Version 2 (2026-11-27, for review): the unit is the scheduler's lever

Joel, 2026-11-27: "need far more formalized architecture... consistent across concerns so it's unified and easy for
top level or even the same sage components to manage", then: "take a look at what we already built. We had worker
thread data flow driven event, publish subscribe". Version 1 already is that form. This week's failures are the
places where a concern stepped outside it, plus one thing version 1 never said: how much one run does.

**What version 1 already gives every concern, and it stays the one form:**
- the worker, its bounded intake, the bus (`State` latest-wins, `Stream` every event), `Need`, `Presence`, `rest`
  and timers (`component::Component`, SAGE.md);
- the class: `Need`'s `Urgency`. `Want` and `Urgent` are held to `Urgency::act_bound`; `Content` (learning,
  dreaming) is held to its queue kept in check, never a per-item bound;
- the run: `triggers`, `cost` (execution and residency, separate claims on the one authority), `run`,
  `RunOutcome` (this document, section 1);
- the stop: `ControlRequest` at yield points and the receipts that prove it (`Yielded.residency_kept`,
  `Cancelled.committed_effects`), and commit-once acknowledgement of what a learn step consumed (section 2);
- space: `place::Resident`; time on the Core: `slice`; choices with history: `decide`.

**What this week showed (receipts).** Each defect is a concern doing its own version of something above:
- Learning a reply had five entries (plain chunk step, whole graph, explicit tile, automatic tile, scattered
  refusals). The plain split, valid only because it was a mode, left every live reply past `n_ubatch` (2048 on the
  M5) learning a partial gradient: cosine 0.960 / norm 0.944 on Qwen3.6-35B-A3B, 0.949 / 0.889 on Qwen3.5-0.8B
  (fork 1eb66d1db, M5 Metal). A sixth entry (398bf318d) at once broke the whole-graph caller (Codex).
- A tile that did not fit failed its step ("learn this reply at a smaller tile"), a refusal for size.
- Units ran longer than their tile (the prefix decode, the stride replay) until pending spans bounded them (fork
  fa85c3379); each run's work was reported only after the fact (`llama_opt_learn_last_unit`, fork 1eb66d1db).
- Shares were constants (`slice::weight`: 1, 2, 4).
- About ten lease and grant types each decide alone whether their work can run (`GrantLease`, `KvSlotLease`,
  `HoldLease`, `CoveringLease`, `StickyLease`, `LaneLeaseTerms`, `NodeGrant`, `RegisteredGrant`, `ThroughputJob`).
- Version 1's control half was never built: no `ControlRequest` is read outside SAGE, `Dispatched` has no task
  owner, and no `Resident` is implemented.

**The amendment: a run's size is chosen by the scheduler, not by the stage.** Version 1 fixes the chunk ("one
engine learn step, sized by measurement"). A stage whose work divides now declares how, and runs what it is given:

```rust
/// How a stage's work divides, in its own quantity (rows, tokens, bytes). Part of `Cost`.
pub struct Lever<U> { pub least: U, pub most: U, pub left: U }

/// `Cost` gains the lever and the residency of one unit of a size, from the stage's own measure:
fn cost(&self, revisions: &[InputRevision], unit: U) -> Cost;   // scratch for that unit beside what stays resident

/// The authorized capability in `StageContext` names the unit it was admitted for. `run` does at most that much,
/// and `RunOutcome::Produced` / the dispatched completion carry what it did:
pub struct Did<U> { pub did: U, pub lasting: Lasting }          // lasting: what survives (below)
```

The rules this adds, each from a receipt above:
1. **A special case is a unit size, never a mode.** A whole-reply graph is `unit >= left`; nothing else enters a
   stage's work. The fork's learn becomes begin, cost at a unit, run a unit.
2. **A unit is bounded and declared.** `did <= unit`, asserted where the completion lands.
3. **No refusal for size or pressure.** A unit whose claim cannot be granted is offered smaller, down to `least`,
   then waits in its class. Only "never here" is typed, decided when the stage is seated: a layout it cannot run on
   (the walk takes her own stream).
4. **Lasting is what is counted.** A learn step's work is its optimizer update: provisional per distinct update id
   until its generation commits, converted once, written off on restore or cancel (Cormac and Codex, 2026-11-27).
   Units advanced are never a reward, so cutting the same work finer never looks like more.
5. **Exactness is checked against itself.** Where a result must not depend on the unit (learning), the gate is
   `unit = left` against `unit = k` on the same backend, at the model's measured floor, plus one cross-backend
   reference.

**The scheduler is the parts that exist, composed once.** Per capacity (the Core first): `slice` picks which ready
stage runs from `Need`; `decide` picks its unit inside the envelope (its time within every bounded class's slack);
the one authority grants the claim, which is the kernel `ResourceDaemon` on `PlacerCore`, never a second ledger
(Cormac). A claim is held per unit size and resized when the size changes, not asked per run (Cormac: per-step asks
cost more than they save). The time a unit takes is fitted by the scheduler from completions; the stage supplies
the quantities that make equal row counts different work (context decoded, replayed rows, trained rows, bytes
moved), and lane occupancy is priced through the fence (Codex). Shares per class are `decide`'s output from
feedback, which deletes `slice::weight`.

**The instances, each a `Component` whose long work is a stage run:**

| Concern | Unit | Lasting | Resident between units | Deletes |
|---|---|---|---|---|
| A mind decoding | a step: her token, or a batch composed from several minds' slices (one dispatch, claim and fence; completions attributed per mind; one mind's cancel never retires the others, Codex) | her turn's tokens within the bound | her KV and recurrent state | the turn-long hold |
| Her prefill | rows up to `n_ubatch` | rows cached | her KV | `prefill_rate`'s private fit |
| The learner | tile rows; the tile boundary is its yield point | an optimizer update (rule 4) | checkpoints, the gradient accumulator, cursor, optimizer (`Yielded.residency_kept`) | the fork's five entries and its "tile did not fit"; the driver's whole-reply path, `measure_defect`, a seat refused for room; `learning/owed` becomes its published state |
| A link's streamer | one block | a block landed before its use | blocks in flight | its own pacing |

**The order, one stacked PR each, each deleting more than it adds:**
1. `Lever`, the unit in the capability, and `Did` in `dataflow`; the composed scheduler in `slice` with `decide`
   setting shares. Validator: `slice::sim` with a bounded and a content class; the content queue stays in check
   while no bounded stage passes its bound, and a unit that cannot be claimed runs smaller.
2. The fork's learn collapses to begin / cost at a unit / run a unit. `test-walk-exact` gates rule 5. Metal, CPU,
   CUDA on the 0.8B; the 35B on Metal (the M5's production form).
3. The learner stage on the Core, with version 1's control half built for it first (yield at the tile boundary,
   cancel with committed effects). Gate: a loaded window on the M5 with the 35B, the learner's queue in check, no
   resident past her bound.
4. Prefill and decode as stage runs with units. 5. The streamer. 6. The lease types fold into the authority's claim.

**Question for review.** SAGE has three worker parents: the raw `Stage` (a queue thread), `Component` (which stands
on it), and `dataflow::Stage` with `StageRunner` (recall). Should `dataflow::Stage` become what a `Component` does
for long work, so a concern implements one parent?
