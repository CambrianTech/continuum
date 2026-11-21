# 3. The roots

[The object model](README.md) · previous: [Principles](02-principles.md) · next: [The lineages](04-lineages.md)

Every type in the core descends from one of these roots. They are few on purpose: each is a concept
the whole core shares. They live at the bottom of the crate layers, so every crate above can descend
from them (chapter 8).

| Root | Kind | Module | Replaces (chapter 1.3) |
|---|---|---|---|
| `Part` and its subsystems | pure trait, plus generic owners | `continuum-base::part`, the worker in `continuum-flow` | the separate roots of every long-lived family; three lifecycles; hand-written child loops |
| `Command` | pure trait | `continuum-base::part::command` | seven single-command dispatchers |
| `Policy` | pure trait | `continuum-base::policy` | eight decision traits |
| `Check` | pure trait | `continuum-base::check` | seven judging traits |
| `Sink` | pure trait, plus generic owners | `continuum-base::sink` | nine recording traits |
| `Source` | partial trait | `continuum-context::source` | five view-feeding traits |
| `Durable` | partial trait | `continuum-saved-state` | eight persistence traits |
| `Demand` | concrete type, read by policies | `continuum-base::demand`; cleared by the governor in `continuum-kernel` | the request side of about 150 Lease, Permit, Grant, Gate and Admission types |

## 3.1 `Part`

A part is anything that runs: it has a lifetime, owns state, and does work.

```rust
pub trait Part: Send + Sync {
    fn identity(&self) -> Identity;     // its kind: a static name and a sentence
    fn generation(&self) -> Generation; // this incarnation; a replacement gets a new one
}
```

`Part` is pure. Its subsystems are where the sharing happens, each written once:

- **`Parts<K, P>`** (`part/parts.rs`), a generic owner. It holds one part per key in order, and its
  traversals implement aggregate, broadcast and select dispatch (2.4). A fixed set of named parts
  (a persona's branches) is declared with the `parts!` macro and traversed by the same code.
  Replacing a key keeps its place and gives the new part a new generation; the old part finishes
  its own shutdown under its old generation.
- **The worker** (`continuum-flow::worker`), a generic owner around the flow stage (CBAR's
  `QueueThread`): a data lane and a reserved control lane, typed topics with revisions and
  provenance, short turns, QoS classes. A worker is a logical owner; many share an executor. An
  event published to a topic may carry a scope (the room it belongs to), a salience and a decay, and
  reaches only that scope's subscribers (chapter 0, "Stimuli are scoped").
- **`Completion<A>`** (`part/completion.rs`), concrete. It records a command's answers as they arrive,
  bound at ingress to an operation id, the target's generation and the parts it admitted. A retry
  rejoins it; an observer can detach and reattach.
- **`Outcome<P, E>`** (`part/outcome.rs`), concrete: `Done`, `Pending(P)` with the completion that will
  finish it, or `Failed(E)`. `Pending` keeps custody.
- **`Published<S>`** (`part/published.rs`), concrete: a snapshot with its owner's generation and its
  revision. Queries read these; an aggregate over them is telemetry.
- **`Task`** (`part/task.rs`), concrete: long work owned by the part whose lifetime it shares; its
  completion wakes that part on the control lane.
- **The lifecycle commands** (`part/lifecycle.rs`): `Prepare`, `Suspend`, `Resume`, `Shutdown`, four
  commands every part answers, broadcast through its parts.

**Commands go down the tree; events go out.** A lifecycle change does two different things, and
each takes the form that fits it:
- **What must happen to what a part owns is a command to its owner,** answered with a completion
  (release her pages, free a model, suspend her branches). Only a command keeps custody: the owner
  holds the bytes until the answer is final, parts stop before what they serve, and nothing runs
  twice. An event cannot do this: it has no acknowledgement, no order and no single owner, so two
  subscribers might both act on it, or neither.
- **That a transition happened is an event,** published by the owner as it happens ("Aiko is
  suspending", "the node is shutting down"). Her attention notices it, a room shows her away,
  telemetry records it, the governor reprices. Observers react within their own concerns; none acts
  on another part's resources because of it.
- **Scope decides who sees it.** A mind's swap on the Core concerns her and her node; a node's
  shutdown concerns everyone on it. Each event carries the scope it belongs to (chapter 0, "Stimuli
  are scoped").

What it replaces: `ServiceModule`, `Daemon` and `BrainRegion` as separate roots; both `Stage`
traits; persona engine's `Lifecycle` and `Preservation`; every family's own `initialize` and
`shutdown`; every hand-written loop over children.

## 3.2 `Command`

A command is a typed request to change a part.

```rust
pub trait Command: Send + 'static {
    type Answer: Send;
    const DISPATCH: Dispatch; // Aggregate, Broadcast or Select: declared once, here
}
```

`Command` is pure. Its one specialization is the kernel's **`ActionCommand`**: a command that callers
outside the tree address by name, with parameters and an answer that generate the SDK and ts-rs
bindings (`continuum-kernel::command`, today's `sdk_codegen`). Inside the tree, parts send each other
plain commands; at the edge, a `Service` (chapter 4.9) receives action commands.

What it replaces: core's and positron's `CommandDispatch`, the client FFI's and the kernel's two
`CommandHandler`s, `GridDispatch`, the client's `ServeHandler`. The kernel's `DynCommand` and its
blanket impls remain as the mechanical bridge from a typed action command to a dynamic registry.

## 3.3 `Policy`

A policy decides. It reads a snapshot and returns a decision, holds no state of its own, and can be
replaced: by a constant today, and by a learned policy once one beats the constant offline (plan
line 14).

```rust
pub trait Policy<Input, Decision>: Send + Sync {
    fn name(&self) -> &'static str;
    fn decide(&self, input: &Input) -> Decision;
}
```

What it replaces: `AllocationPolicy`, `WindowPolicy`, `GridPlacementPolicy`, `TierDownPolicy`,
`NextPolicy`, `FocusPolicy`, `WorkingSetPolicy` and `BatchScheduler`. Each becomes a `Policy` with
its own input and decision types; their implementors become its leaves.

## 3.4 `Check`

A check judges a subject and reports findings, without changing it.

```rust
pub trait Check<Subject>: Send + Sync {
    type Finding;
    fn name(&self) -> &'static str;
    fn check(&self, subject: &Subject) -> Vec<Self::Finding>;
}
```

What it replaces: source hygiene's `SourceRule`, `CrateRule` and `Shape`; hands' `SyntaxValidator`;
cognition's `PerceptionFact`; engram's `RedactionDetector`; academy's `SalienceDetector`. Each keeps
its subject type (a source file, a crate, a reply, a turn) as the generic parameter.

## 3.5 `Sink`

A sink records events for capture, replay or measurement, and never blocks its caller.

```rust
pub trait Sink<Event>: Send + Sync {
    fn record(&self, event: Event);
}
```

Generic owners give every sink its behavior once: `Noop`, `Ring` (bounded memory), `Jsonl` (a file),
and `Fanout` (several sinks). What it replaces: base's `CaptureSink` (renamed) and the eight
family-specific record traits; each family keeps only its event type.

## 3.6 `Source`

A source feeds a mind's view of the world through the one context codec
([CONTINUUM-ENGINE.md](../CONTINUUM-ENGINE.md) §5). It is partial: it declares its items and their
levels of detail, and the codec supplies caching, ordering and collapse. It already exists
(`continuum-context::source::Source`, ten implementors).

What it replaces: grounding's `RagSource` and `CriticalSource`, positron's `StateSource`, and mind's
`RagRenderable`. Their thirteen implementors move onto `Source`.

## 3.7 `Durable`

A durable value outlives a process. It declares its key, its format and its retention once; the
medium it is stored in (an owned file, an ORM row) is an implementation.

It is partial, built on what exists: `continuum-saved-state`'s `SavedState` (state with a kind, a
format, a retention and a medium) is generalized so that an ORM entity (`OrmEntity`, a row in a
collection) is one more medium.

Key, format and retention are not enough for the callers that rely on durability (Codex's review).
A durable value also declares which of three contracts it keeps:
- **Commit visibility:** a write is either fully visible to the next reader or not at all.
- **Atomic checkpoint with its cursor:** state and the position it consumed through (a journal cursor,
  an event cursor) are saved as one unit, so a restart never re-applies or skips an input.
- **Replay idempotency:** replaying an input that was already applied changes nothing.

A learning checkpoint keeps all three; a cache of derived values keeps none. The medium implements
the contracts a value declares and refuses a value whose contracts it cannot keep.

What it replaces: `LedgerStore`, `RunLedger`, `ActivityStateStore`, `StepStateSink`,
`RecallTraceStore` and `EpochWatermarkStore`. Each becomes a `Durable` value with its own key and
medium, and its store is the medium's generic owner.

## 3.8 `Demand`

A demand is what a piece of work presents to the governor: everything the governor needs to decide
who goes next, for how long, at what quality, and on which node. It is the one interface every
governor in the system decides by: the Core's scheduler choosing the next mind, a node's memory
governor, the time governance of SHARED-RESOURCES, and the grid and mesh markets. The pricing it
carries is specified in [GRID-MARKET-CLEARING.md](../GRID-MARKET-CLEARING.md) §1a: the currency is
weighted time to a verified outcome, and every demand carries its own value over time.

```rust
pub struct Demand {
    pub work: WorkId,                  // the turn, lesson, frame or render it is for
    pub activity: ActivityId,          // whose value curve and weight apply
    pub value: ValueCurve,             // U(t): value of a verified completion at time t; its horizon
    pub essentials: Essentials,        // what must be whole or the result is scrap
    pub quality: QualityLadder,        // the steps it can take down and still deliver
    pub progress: Progress,            // its verified sub-outcomes so far: edits landed, steps committed
    pub completion: CompletionEstimate,// predicted time to complete, measured, with its spread
    pub switch_cost: SwitchCost,       // what freezing and unfreezing it costs: bytes to move, restore time
    pub resources: ResourceNeed,       // memory per tier, device time, network
}
```

What each field is for:

- **Value and activity.** The curve comes from the activity: a live reply decays once the conversation
  moves on, a frame has a hard deadline, a refactor is flat for hours, dreaming is low and never zero.
  The one human input, which activity matters now, weights it.
- **Essentials and quality.** A demand is never granted less than its essentials ("a car missing its
  engine is scrap"). It may be granted a lower step of its quality ladder ("a smaller engine is
  fine").
- **Progress.** Long work enters as verified sub-outcomes, so it is credited as it advances, and a
  preemption never discards it: a frozen demand keeps its progress and resumes.
- **Completion and switch cost.** Together they let the governor price both waiting and switching.
  Freezing a demand costs it the time it waits, never the work it has done: a red light, not a return
  to the start. Each switch costs the bytes moved and the restore time. Hard work with a large
  context has a high switch cost, so once it is warm it is cheaper to let it run to a natural boundary
  than to slice it thinly. The governor sizes each run by comparing the value others lose while they
  wait with the switch cost a shorter run would add.

The governor answers a demand with a typed outcome:
- **`Granted`:** at a quality step, on a node, for a run length;
- **`Waiting`:** with the event that will wake it;
- **`Infeasible`:** before admission, when no grant can meet its essentials before its horizon;
- **`MissedCommitment`:** after admission, when one could not be kept.

A cleared grant is a lease (today's `ThroughputLease` is one).

**The Core is a CPU core for minds.** Joel, 2026-11-21: "We swap ONE agent in per core, fast, often, at
negligible sub second time to do so, all its memory like a cpu core, hence its name. If you have math
dividing up context amongst persona it's all wrong." A Core is one model on one compute unit with one
context. One mind is swapped into it at a time and has all of it; the others are saved, as a process's
registers are, and swapped back in a fraction of a second (a switch measured at about 21 ms between
residents, 0.6). The Core owns, and nothing outside it decides:
- **its context:** one, its capacity measured by the engine, whole to whichever mind is in it (4.11);
- **its occupant:** the mind swapped in now;
- **its cache:** the saved pages of minds recently on it, kept warm while room remains;
- **its next-mind policy** (below).

**Nothing divides a Core among minds, at any grain, anywhere.** No grant, share, lane, seat, slot,
per-mind window, minds-per-anything or count of minds enters a Core's memory or a mind's window. A mind
holds no window between turns; at her turn her window is her need, up to the whole context. A mind asks
for a turn with a `Demand`; the Core answers `Granted` when she is swapped in, `Waiting` with the event
that swaps her in, or `Infeasible` before admission only when her essential need exceeds the whole
context, with both numbers, so placement can move her to a larger Core. Her `Residency` records where her
pages are; only the Core moves them. The node's governor shares the device between kinds of work (the
Core, rendering, speech, embeddings, learning) by demand, never among minds; when the Core's share
shrinks it gives back cached pages first and its occupant's context last.

**One Core per model per compute unit: for Qwen on the M5, one.** Decode is bound by memory bandwidth:
every token reads the model's active weights, so a second Core of the same model on the same device
reads them twice through the same bandwidth and splits the rate, and batching minds inside one Core did
no better (four sequences per decode gave each about 2 tokens per second against the full rate for one
mind at a time, `persona_scheduler` deployed 6404cf3b15). With a sub-second swap, a second Core buys no
latency either: a live reply swaps in at a token boundary. Speed for many minds comes from swapping
fast, never from running them side by side. A second Core exists only for a different model doing a
different job for the mind (a small fast model behind her quick speech layer, a mind subcomponent, all
event-driven); that is designed when it is built, and it is not now.

**Custody through a swap** (Codex's review of 4a5807f2cc):
- **The occupant keeps her cells until her release.** Her context is hers until her native boundary
  and release receipt; the next mind restores into cells she is not using.
- **A page leaves the device only when its save is confirmed.** A waiting demand is woken by that
  confirmation, never by a timer or an estimate.
- **One publication.** The Core publishes one generation-tagged snapshot (capacity, occupant, the
  cache's residency), and every reader reads that: deliberation fits her view to the capacity, health
  reads the snapshot, nothing keeps a second idea of a mind's window.
- **Replays that must pass:** a delayed and a duplicated release; an 84k demand arriving while a
  smaller next turn waits.

What it replaces: every per-mind division in serving and governance, listed in 4.11 ("Governance has
nothing to divide") and chapter 11 phase D: the serving daemon's per-mind grants and their "owed" state,
the window allocator's sizing of minds, lanes and the lane planner, the decode knee as a lane count,
seats bounded by lanes, and the engine's own split of its context into streams (Aris, the M5,
2026-11-21: owed a window for 25 minutes while one mind held 98,304 tokens of a Core that decodes one
mind at a time).

**Cores elsewhere.** A Core on another node is a Core; a mind reaches it through a `Session` (chapter
4.1) and the market (GRID-MARKET-CLEARING). Moving her pages across the network costs far more than a
local swap, so remote placement favours minds whose context is already there or cheap to rebuild.

**Which models, how many, and when they are awake.** Models are a population like genes, at a coarser
grain: each has a fitness and a residency priority, is loaded on demand and released when idle. No
menu of kinds or sizes is configured.
- **A node starts with one.** One base model and one Core: the largest model whose weights and one real
  turn fit the Core's share, measured at startup. One base serves the resident as patient, doctor and
  dream ([ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](../ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)).
- **Kinds come from capability gaps.** Generation, embedding, speech in and out, vision, a small fast
  model for quick responses: a kind is added only when demand needs a capability the base serves badly
  or too slowly for its deadline, as a gap in her skills grows a gene (chapter 6.4).
- **Size is the largest that fits.** Each model is the largest that fits its compute unit with one real
  turn. A smaller variant of a kind exists only where a deadline needs it (a call's quick voice) and
  measurement shows the larger one misses it.
- **How many follows demand.** One model per capability per compute unit by default (the Core rule
  above); more only where demand pays for them.
- **Models have presence levels.** Loaded on a Core, warm in RAM or flash, cold on disk or a hub. A
  demand event promotes a model, idleness or pressure demotes it, and its load time counts as its
  switch cost, so models do not thrash.
- **The grid shares them.** A node does not load a model the grid already serves cheaply; a peer's
  model is a remote Core, priced by the market. A grid that starts as one node with one model grows
  kinds where demand and fitness say so.

**Choosing the next mind on the Core** is the same rule at its finest grain. Among the ready demands,
run the one whose expected gain in value per unit of Core time is highest, counting the switch it
would cost. Run it until its next natural boundary or until another demand's loss from waiting
outweighs the switch, whichever comes first. Every resident is time-shared this way. In a call with
several residents, the one speaking now holds the highest curve. In a room of programmers, a hard
change already warm runs to its next landed edit before the Core moves on. The curves' shapes and the
conversions are measured and learned (GRID-MARKET-CLEARING §1a, "What is measured and what is
learned"), never fixed constants.

**Demands compose through the tree.** An activity's demand is made of its residents' demands, and a resident in several activities presents a demand in each; a
resident's is made of her branches'; and inside her context, every source (her history, the
grounding, her tools, her results, a room's view) is a demand on her context window. It is the same
object at every grain, cleared by the same solver (`cambrian-flex`, the one flex allocation of
[ONE-BUDGET.md](../ONE-BUDGET.md)):
- the activity tier prices its residents' turns of the Core;
- a node's governor prices memory and device time across residents;
- her context prices its sources against her window.

Each tier quotes a price to the tier below it, as GRID-MARKET-CLEARING §2 nests them. Nothing is
divided by a fixed split at any grain, and nothing is compressed in batches: items stream in and step
down their own levels of detail (chapter 0, "Every object is a chain of stand-ins"), and the newest
interaction is always whole, because it is essential.

**Focus is a publication, and her context subscribes to it.** Her attention region publishes her
focus whenever it changes (chapter 5.2). Every source in her context reads that publication and
re-prices its items: recall pre-stages what her new focus will need, a room she is no longer
watching steps down to its headline, the file she just opened comes up whole. Nothing recomputes her
whole prompt per turn; only the sources her focus moved past change, and the rest stay cached.

**Her context budget is her focus.** A resident can be given the whole pool when her work needs the
breadth. Giving her less is not a shortage to apologise for; it is concentration, and often the
better choice: what does not earn its place at her current focus steps down to a headline she can
expand. The budget follows her attention and the activity's value curve, moment to moment, never a
constant written into code.

**Kept explicit** (Codex's review): a demand states its prerequisites (what must already be true or
resident) and any contractual deadline it carries, rather than leaving them implied by its curve.
Switch costs are measured, including the warm context a switch would discard or retain. A waiting
demand is woken by a confirmed release of what it needs, never by a timer or an estimate.

What it replaces: every request type that today asks for a resource in its own vocabulary. That is
the window allocator's needs, the `LeaseRequest`s, admission gates and their permits, and the
serving planner's demands. The answers they receive become leases and typed outcomes on one
governor (Codex's lineage, chapter 4.3).
