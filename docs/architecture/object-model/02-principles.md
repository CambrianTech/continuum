# 2. Principles

[The object model](README.md) · previous: [The core today](01-today.md) · next: [The roots](03-roots.md)

Nine principles shape every type in this book. Each says what to do and why.

## 2.1 Lineage: each generation holds only its difference

Joel: "machine -> automobile -> truck -> F150, not just machine -> F150." A type descends from the
most general abstraction that describes it, through every generation in between. Each generation
adds one thing; everything shared by several descendants is written once, at the narrowest ancestor
they all share. Skipping a generation is the defect: when a leaf implements a root directly, the
shared middle is rewritten in every leaf. That is what chapter 1.3 measures.

**Finding a parent.** Read the code as language. List the nouns (types) and the verbs (methods), and
ask each word for its parent word, as language does: a truck is an automobile, a throw is an action.
Whisper, Kokoro and Silero are each a model; a lane capture sink and a placement capture sink are
each a sink; `backoff_after` and `retry_delay` are each a backoff. A shared parent word is a shared
base.

## 2.2 Abstractions are pure, partial, or concrete

- **A pure abstraction** is a trait with only required methods. It is a contract: every implementor
  supplies every answer. Roots are usually pure (`Part`, `Policy`, `Sink`).
- **A partial abstraction** supplies part of the answer once. In Rust it takes one of two forms:
  - a trait with default methods, when every implementor may safely inherit the default;
  - a **generic owner**: a struct generic over a narrower trait that implements the broader behavior
    once. `Loaded<M: Model>` implements a model's lifecycle once for every model; the leaf supplies
    only how to load.
- **A concrete type** is a leaf: one model, one transport, one region.

A default body that a stateful implementor must not inherit (a default `Done` for a release) is
never written. Mandatory sequencing lives in a generic owner's method with private state, because a
trait default can be overridden.

## 2.3 Generic owners, not blanket impls

Rust rejects two blanket impls that could overlap (`impl<T: Model> Lifecycle for T` beside
`impl<T: Composite> Lifecycle for T`). A generic owner gives the same sharing without the conflict,
and keeps the shared state (a model slot, a set of parts, a completion) in one named place. Blanket
impls are kept only where one trait is mechanically derived from another (the kernel's command
system, 1.2).

## 2.4 Composition: parts are made of parts

An object owns its sub-objects, and any method on the parent reaches its parts the same way. Every
method that crosses into a part's parts declares its dispatch where it is declared:

- **aggregate:** ask every part and combine the answers (footprint, health);
- **broadcast:** send to every part and keep each answer (shutdown, suspend);
- **select:** send to one part chosen by the parent's policy (synthesize: one voice speaks).

The traversal for each dispatch is written once. Direction follows dependencies: starting reaches the
parent before its parts; stopping reaches the parts before the parent.

## 2.5 Data flow: workers, commands, publications

Every part lives on a worker: its logical, serialized owner. Typed inputs wake it, it takes a short
turn, and it publishes what it produced with provenance, which wakes exactly what depends on it.
- **Queries** read a part's publications; they never wait on its worker.
- **Commands** change a part; submitting one returns its completion at once, and each answer is
  recorded in the completion as it arrives.
- **Tasks** carry long work; a task's completion is an input that wakes its part again.

Nothing calls into a part and waits for it to finish. Nothing polls. A cadence is only a floor for
a part that must report when nothing changes.

## 2.6 Ownership is not location

A part owns its state and the authority to change it. Another part may hold the bytes (the engine
holds a persona's KV, the store holds her rows), and the owner holds a handle to them, never a copy.
State in a `static` or a global is a missing object: the object that should own it does not exist yet.

## 2.7 One name per concept

A concept has one name everywhere: in the code, in this book, in commands and in logs. A second
trait with the same verbs as an existing one is a second lineage, and the existing one is extended
instead. A name that collides with another crate's name for a different thing is renamed. Names
describe what a thing is in the domain (a part, a model, a transport), never how it was built (a
module, a daemon, a handler).

## 2.8 Measured, never written: no number is for one machine

The repo runs on hardware nobody here owns: a laptop with unified memory, a discrete GPU an owner also
games on, a phone, a grid node. A number written into the code or this book is one machine's
measurement applied to all of them (1.5 lists thirty, from `STREAMS = 2` to a 25-minute act deadline).
So a bound is a type, not a literal:

- **`Measured<T>`** is a value with its probe, the hardware and model it was taken on, its time and its
  spread. Policies take `Measured` inputs. A literal cannot be passed where a `Measured` is expected,
  so a constant is a compile error, not a review comment.
- **`Prior<T>`** is what a policy uses before its own measurement exists: learned from the population
  (other nodes and other minds measured on the same kind of hardware and model), carried with that
  provenance, and replaced by the first local measurement. A prior is data, never a literal.
- **The hardware is a descriptor, measured.** Each `ComputeUnit` (a GPU, the Neural Engine, a CPU) is
  probed when it is discovered and again when its model, backend or driver changes: decode and prefill
  rates by context length, model load and wake times per tier, page save and restore rates, whether two
  of its workloads together beat one swapping. This is RAGE's tunables-as-data (chapter 0, "Missions
  are scripts"), with the data measured on the console it runs on instead of authored for one.
- **A receipt is not a rule.** "On the M5, two sequences decoding split the rate" is that unit's
  measurement and stays in this book as evidence; the rule is that each unit acts on its own.

## 2.9 Everything streams, end to end

A token, a percept, a receipt and a page move as events from the part that produced them to every part
that consumes them, and nothing in between collects them whole. A seam that accumulates a stream and
hands it on at the end turns a live voice into a recording and a mind's early thought into a late one.
A stream ends with a typed end that says why (2.5), never a count someone compares afterwards, and the
producer's measured facts (how many tokens, how many reused, how long) travel with it, so no consumer
estimates what was measured.
