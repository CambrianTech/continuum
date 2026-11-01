# The stage contract (M0), version 1

Status: DESIGN FOR REVIEW (2026-11-01). One versioned contract before parallel implementations
(Codex's coordination, airc 1c9c2644). Owners: Fable (this contract; perception and reflex
migration), Cormac (the dream and learning consumer; chunk yield and wake semantics), Codex
(shared task lifetime and terminal delivery; admission and the physical grant). Each owner
reviews the other two boundaries. THE-WHOLE-MIND §1b is the why; this is the what.

## 1. Types (owned by `continuum-kernel/src/runtime/stage/`)

```rust
/// A brain function's identity, stable across restarts.
pub struct StageId(Uuid);
/// One running instance of a stage for one mind. A new incarnation (restart, migration,
/// re-host) fences every grant, ack and completion of the previous one.
pub struct StageIncarnation { pub stage: StageId, pub mind: MindId, pub epoch: u64 }

/// What a stage consumes: typed topics, routed by header, never by body.
pub struct InputTopic(&'static str);
/// The exact input a run saw: the topic and the revision (or replay cursor) it read through.
pub struct InputRevision { pub topic: InputTopic, pub revision: u64 }

/// What a stage produces, with provenance: which incarnation, from which input revisions.
pub struct Provenance { pub by: StageIncarnation, pub from: Vec<InputRevision> }

/// When a stage runs: an input revision moved, a salience threshold crossed, a task it
/// dispatched completed, or a cadence floor (the never-comatose floor, never the main trigger).
pub enum Trigger { Input(InputTopic), Salience { topic: InputTopic, threshold: Salience }, Completion, Floor(Duration) }

/// What a run costs: none (CPU only), or a model call at a tier. Physical capacity is claimed
/// through the one resource authority (`resources::LeaseRequest` -> `LeaseGuard`); the
/// stage scheduler only ranks ready demand, it never holds capacity itself.
pub enum Cost { Cpu, Model { tier: ModelTier } }

/// What a running stage may be asked to do at its yield points, and what each means.
pub enum Control {
    /// keep going
    Continue,
    /// stop at the next yield point, keep all state and its grant; resumable as the same run
    Pause,
    /// stop at the next yield point, release the grant, keep state; re-admitted later
    Yield,
    /// end the run; any effect not yet committed is discarded and reported as discarded
    Cancel,
}
```

A stage implements:

```rust
#[async_trait]
pub trait Stage: Send + Sync + 'static {
    fn id(&self) -> StageId;
    fn inputs(&self) -> &[InputTopic];
    fn trigger(&self) -> Trigger;
    fn cost(&self, input: &[InputRevision]) -> Cost;
    /// One run. Long work is a task (the shared task owner) whose completion re-triggers.
    async fn run(&self, ctx: &StageContext, input: &[InputRevision]) -> RunOutcome;
}

pub enum RunOutcome {
    /// outputs to publish, each carrying Provenance
    Produced(Vec<Output>),
    /// dispatched work as a task; completion arrives through the task owner (Codex)
    Dispatched(TaskRef),
    /// nothing to do for these revisions
    Idle,
    /// stopped at a yield point under Control; says which
    Stopped(Control),
}
```

`StageContext` carries the incarnation, the controls channel (checked at yield points), the
grant guard when one was admitted, settings as commands (THE-WHOLE-MIND §1b: every stage's
state and settings are commands), and probes and capture.

## 2. Contracts at the three boundaries

- **Task lifetime and delivery (Codex).** A dispatched run's completion and acknowledgement go
  through the shared task owner (#5585's terminal custody): completion is durable before it is
  delivered, delivered at least once, and accepted idempotently by task identity. A late or
  duplicate completion for a fenced incarnation is refused with a typed outcome.
- **Admission and the physical grant (Codex).** One authority (`resources::daemon`) grants
  physical capacity. The stage scheduler ranks ready demand by each mind's state (arousal,
  engagement, age; THE-WHOLE-MIND §4.2), never holds a FIFO in front of a mind, and never
  grants more than the authority admitted.
- **The dream consumer (Cormac).** Dreaming is a stage whose cost is a model tier and whose
  runs are chunked: each chunk is a yield point, so Pause and Yield land within one chunk (the
  wake latency). Its inputs are the settled-experience topic and her dream-share setpoint.

## 3. First migrations (Fable)

- **Recall (outlier A):** today `recall_faculty` wrapped in `deferred_faculty`. It becomes a stage
  whose input is her perception revision, whose cost is `Cpu`, and whose output is a recall track
  with provenance.
- **Reflex (outlier B):** today `directed_pending` plus `TurnAttention::for_message` at four
  sites. It becomes a stage whose input is the perception topic (from the store, PERCEPTION-
  FROM-THE-STORE.md), whose cost is `Cpu`, and whose output is the addressed trip
  (`PersonaIdentity::addressed_by`, species-blind).

Deleted when their migrations land: the `BrainRegion::tick` cadence-only interface (regions
move to `Stage` with a `Floor` trigger), `deferred_faculty`'s wrapper, `directed_pending`'s
global map, the four per-site attention derivations.

## 4. The shared validator (all three owners)

Recall, reflex and dream under contention on one replayed engine: a stimulus during work (the
reflex trips; the dream yields within one chunk), late and duplicate completions, a restart in
the middle of each, a stale incarnation's ack refused, and subsequent useful work after each
case (the mind keeps working, both ways: law 8).
