# The stage contract (M0), version 1

Status: DESIGN FOR REVIEW (2026-11-01), revision 2 with Codex's boundary review at eeff13c3. Not
frozen until Codex and Cormac pass it. One versioned contract before parallel implementations
(Codex's coordination, airc 1c9c2644). Owners: Fable (this contract; perception and reflex
migration), Cormac (the dream and learning consumer; chunk yield and wake semantics), Codex
(shared task lifetime and terminal delivery; admission and the physical grant). Each owner
reviews the other two boundaries. THE-WHOLE-MIND §1b is the why; this is the what.

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
