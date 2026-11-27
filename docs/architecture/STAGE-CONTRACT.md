# The stage contract

How a concern that works on a shared capacity runs: what wakes it, what it costs, how much one run does, how it is
stopped, and what it publishes. Owners: Fable (this contract, the learner as its first full consumer), Codex (task
lifetime and delivery, admission and the physical grant), Cormac (the learning consumer, the authority side).
THE-WHOLE-MIND §1b is the why.

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [Where it stands](#where-it-stands)
- [The types](#the-types)
- [Units: how much one run does](#units-how-much-one-run-does)
- [Control and its receipts](#control-and-its-receipts)
- [Publication and dataflow](#publication-and-dataflow)
- [The scheduler and the authority](#the-scheduler-and-the-authority)
- [Consumers](#consumers)
- [The shared validator](#the-shared-validator)

## Where it stands

- **Built** (`core/continuum-sage/src/dataflow/`): identity, topics and provenance, `Triggers`, `Cost`, the `Stage`
  trait and `RunOutcome`, the publication compare-and-set, artifacts and dependent stages, and `StageRunner` for
  self-contained stages. Its one production consumer is recall (`continuum-cognition/src/cognition/deferred_faculty.rs`).
- **Not built:** the control half (no `ControlRequest` is read outside SAGE), the task owner for `Dispatched`,
  units, and `ExecutionCost::Learn`. The learner is their first consumer.
- **One parent for a concern** (agreed with Codex, 2026-11-27): a concern implements `component::Component`, and its
  long work is a stage run installed on that component's worker. `dataflow::Stage` is deleted only after one learner
  and a cancellation-during-run test have moved. The raw `sage::Stage` (the intake and overflow worker under
  `Component`) is a different thing and keeps its semantics.

## The types

```rust
/// A brain function's identity, stable across restarts; one running instance for one mind; one run within it.
pub struct StageId(Uuid);
pub struct StageIncarnation { pub stage: StageId, pub mind: MindId, pub epoch: u64 }
pub struct RunId { pub incarnation: StageIncarnation, pub seq: u64 }

/// A topic names its payload and its kind: replayable events (read through a cursor) or a replaceable snapshot.
pub enum TopicKind { ReplayableEvents, ReplaceableSnapshot }
/// The exact input a run saw, captured at dispatch, and what a run produced from which inputs.
pub struct InputRevision { pub topic: &'static str, pub through: u64 }
pub struct Provenance { pub by: RunId, pub from: Vec<InputRevision> }

/// What wakes it, together: an input moved, a dispatched task completed, the never-comatose floor.
pub struct Triggers { pub inputs: Vec<TopicName>, pub completions: bool, pub floor: Option<Duration> }

/// What a run costs: executing and staying resident are separate claims on the one authority,
/// because paused or yielded state still holds bytes after execution is released.
pub struct Cost { pub execution: ExecutionCost, pub residency: ResidencyCost }
pub enum ExecutionCost {
    Cpu { measured: Option<Duration> },   // never claimed free, never a guessed number
    Model { tier: ModelTier },
    Learn { share_ppm: u32, period: Duration },  // proposed: a share of one lane's engine time
}
pub struct ResidencyCost { pub host_bytes: u64, pub device_bytes: u64 }

#[async_trait]
pub trait Stage: Send + Sync + 'static {
    type Input; type Output;
    fn id(&self) -> StageId;
    fn triggers(&self) -> Triggers;
    fn cost(&self, revisions: &[InputRevision]) -> Cost;
    async fn run(&self, ctx: &StageContext, input: &Self::Input, revisions: &[InputRevision]) -> RunOutcome<Self::Output>;
}

pub enum RunOutcome<O> {
    Produced(Vec<(O, Provenance)>),  // published under the publication rule
    Dispatched,                      // long work handed to the task owner; its completion re-triggers the stage
    Idle,                            // nothing to do for these inputs
    Stopped(StopReceipt),            // stopped at a yield point; the receipt proves which stop
}
```

The class of a stage's work is its component's `Need`: `Want` and `Urgent` are held to `Urgency::act_bound`;
`Content` (learning, dreaming) has no per-item bound, and its guarantee is its queue kept in check.

The context carries the authority's typed capability for the admitted execution, residency and unit, never a
guard whose lifetime ends silently when `run` returns.

## Units: how much one run does

Proposed. A stage whose work divides declares how, and the scheduler chooses how much each run does:

```rust
/// How its work divides, in its own quantity (rows, tokens, bytes).
pub struct Lever<U> { pub least: U, pub most: U, pub left: U }
/// The cost of one run at a unit: its scratch beside what stays resident, from the stage's own measure, plus the
/// quantities that make equal row counts different work (context decoded, rows replayed, rows trained, bytes moved).
fn cost(&self, revisions: &[InputRevision], unit: U) -> Cost;
/// What the run did, carried by `Produced` and by a dispatched completion.
pub struct Did<U> { pub did: U, pub lasting: Lasting }
```

1. **A special case is a unit size, never a mode.** The whole of a piece of work is `unit >= left`.
2. **A unit is bounded and declared:** `did <= unit`, asserted where the completion lands.
3. **No refusal for size or pressure.** A unit whose claim cannot be granted is offered smaller, down to `least`,
   then waits in its class. Only a layout the stage cannot run on is typed, and it is refused when the stage is
   seated, before any run.
4. **What lasts is what counts.** A learn run's work is its optimizer update: provisional per distinct update id
   until its generation commits, converted once, written off on restore, cancel or retry. Units advanced are never
   a reward, so cutting the same work finer never looks like more (Codex, Cormac).
5. **A result that must not depend on its unit is checked against itself:** `unit = left` against `unit = k` on the
   same backend, at the model's measured floor, plus one cross-backend reference.

Why (the 2026-11-26/27 receipts): learning a reply had five entries, and the plain split, valid only as a mode, left
every live reply past the Core's `n_ubatch` learning a partial gradient (cosine 0.960 on the 35B); a tile that did
not fit failed its step; units ran past their tile until each was bounded; work was reported only after the fact
(IN-PROCESS-LEARNING.md).

## Control and its receipts

Not built. A request is not a stop; only a receipt from the run's owner proves which stop happened. Receipts and
continuations are owned by the task owner across a caller returning, a forgotten handle and a restart.

```rust
pub enum ControlRequest { Continue, Pause, Yield, Cancel, Shrink { to: ResidencyCost } }
pub enum StopReceipt {
    Paused { run: RunId },                                        // execution and residency both stay charged
    Yielded { run: RunId, residency_kept: ResidencyCost },        // execution released; what stays is named
    Cancelled { run: RunId, committed_effects: u64 },             // terminal after quiescence
    Refused { run: RunId, refusal: StopRefusal },                 // PauseUnsupported, AlreadyTerminal, StaleIncarnation
}
```

- A request reaches a running unit at its yield point even while the component's handler holds it; a cancel queued
  behind that handler is not a timely stop (Codex).
- A shrink of a stage's residency arrives as the same request at the next yield point. The kernel's elastic hold
  and `ShrinkHolder` (`cormac/step3`) are its adapter onto the authority, never a second path (Cormac). For the
  learner, a shrink above the floor tile is answered by a smaller unit; below it, it checkpoints and yields.
- `Dispatched` work completes through the existing task owner, correlated by run id: durable before delivered,
  delivered at least once, accepted idempotently. A completion from an old incarnation stays in the ledger and the
  current incarnation adopts or disposes of it with a receipt; fencing blocks stale mutation, never evidence.
- For the learner: the yield point is the unit boundary; `Yielded.residency_kept` is the shadow, optimizer,
  checkpoints and accumulator; `Cancelled.committed_effects` is optimizer updates committed. If the engine's
  incarnation changes mid-unit, quiescence is proved by the fence (the unit stamped with its execution and engine
  incarnation, and its committed step read back), never by waiting; without that evidence the run stays unresolved
  with its custody retained.

## Publication and dataflow

Built. Within one incarnation, overlapping runs publish with a compare-and-set on the input revisions they derived
from: an older run never overwrites state derived from newer input.

A stage emits what it delivers and whatever needs it subscribes (`dataflow::artifact`):
- `artifact::<T, F>(topic)`: a produced-once value; consumers hold a cloneable `Subscribes` and receive `Arc<T>`.
  `handoff` is the one-consumer form that moves the value. Every artifact settles once: `Ready`, `Failed(F)`, or
  `Abandoned` when its producer is gone, so no waiter waits on nothing.
- A `DependentStage` runs once when the last artifact it subscribes to settles (`spawn_when_ready`), with its own
  task, its output as an artifact, and its timing probed.

Consumers: prefill during the tool (`cognition/llm_deliberation_faculty/prefill_during_tool.rs`) and the
endpoint-retained page IO's result (`inference/turn_admission/paging_lifetime.rs`).

## The scheduler and the authority

Per capacity, the Core first:
- `slice` picks which ready stage runs, from each component's `Need`, by stride shares with aging;
- `decide` picks its unit inside the envelope (its time within every bounded class's slack) and sets the shares per
  class from feedback, which deletes `slice::weight`'s constants;
- the one authority grants the claim: the kernel `ResourceDaemon` on `PlacerCore`, never a second ledger (Cormac).
  A claim is held per unit size and resized when the size changes, never asked per run;
- a unit's time is fitted by the scheduler from completions, with lane occupancy priced through the fence (Codex);
- a batch of decodes is composed from several minds' slices: one dispatch, claim and fence, completions attributed
  per mind, and one mind's cancel or lateness never retires the others (Codex).

The lease and grant types that each decide alone whether their work runs (`GrantLease`, `KvSlotLease`, `HoldLease`,
`CoveringLease`, `StickyLease`, `LaneLeaseTerms`, `NodeGrant`, `RegisteredGrant`, `ThroughputJob`) fold into the
authority's claim as their callers become stage runs.

## Consumers

| Concern | Unit | Lasting | Resident between units | State |
|---|---|---|---|---|
| Recall | one run | its published track | none | built (`StageRunner`) |
| The learner | tile rows | an optimizer update | shadow, optimizer, checkpoints, accumulator | first for units and control (IN-PROCESS-LEARNING.md) |
| A mind decoding | a step: her token, or her slice of a batch | her turn's tokens within the bound | her KV and recurrent state | lane today (THE-LANE-ON-SAGE.md) |
| Her prefill | rows up to `n_ubatch` | rows cached | her KV | `driver/prefill_rate.rs` until moved |
| A link's streamer | one block | a block landed before its use | blocks in flight | `continuum-blocks` until moved |
| Reflex | one run | the addressed trip | none | to move from `directed_pending` |

Order: units and the composed scheduler in SAGE (validator `slice::sim`: a content queue in check while no bounded
stage passes its bound; a unit that cannot be claimed runs smaller); the fork's learn collapsed to begin, cost at a
unit, run a unit; the learner on the Core with the control half built for it; then prefill and decode, the streamer,
and the lease fold. Each PR deletes what it replaces.

## The shared validator

Recall, reflex and the learner under contention on one replayed engine, each case followed by useful work:
- a stimulus during work: the reflex trips, the learner yields at its next unit boundary, the receipt carrying the
  measured latency for the node class beside the observed one;
- the engine relaunches mid-unit: with the fence's evidence, the cancel receipt carries the proven committed count
  and nothing re-applies; without it, the run stays unresolved with custody retained;
- late and duplicate completions; a late delivery after a restart, adopted with a receipt;
- a stale incarnation's mutation refused while its completion stays adoptable;
- a forgotten continuation reclaimed and reported; a stale run's output refused by the publication rule;
- an unsupported pause refused by type; a shrink mid-run answered by a smaller unit;
- a restart in the middle of each.
