# 3. The roots

[The object model](README.md) · previous: [Principles](02-principles.md) · next: [The lineages](04-lineages.md)

Every type in the core descends from one of these roots. They are few on purpose: each is a concept
the whole core shares. They live at the bottom of the crate layers, so every crate above can descend
from them (chapter 6).

| Root | Kind | Module | Replaces (chapter 1.3) |
|---|---|---|---|
| `Part` and its subsystems | pure trait, plus generic owners | `continuum-base::part`, the worker in `continuum-flow` | the separate roots of every long-lived family; three lifecycles; hand-written child loops |
| `Command` | pure trait | `continuum-base::part::command` | seven single-command dispatchers |
| `Policy` | pure trait | `continuum-base::policy` | eight decision traits |
| `Check` | pure trait | `continuum-base::check` | seven judging traits |
| `Sink` | pure trait, plus generic owners | `continuum-base::sink` | nine recording traits |
| `Source` | partial trait | `continuum-context::source` | five view-feeding traits |
| `Durable` | partial trait | `continuum-saved-state` | eight persistence traits |

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

What it replaces: `LedgerStore`, `RunLedger`, `ActivityStateStore`, `StepStateSink`,
`RecallTraceStore` and `EpochWatermarkStore`. Each becomes a `Durable` value with its own key and
medium, and its store is the medium's generic owner.
