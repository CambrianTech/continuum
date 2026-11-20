# Parts: objects, methods and data flow, one design

Status: design, revised 2026-11-20 after review. Owners: Fable (the part, its methods and data
flow); Codex (resources, completion ownership, review). Builds on the stage contract
([STAGE-CONTRACT.md](STAGE-CONTRACT.md)), the concurrent mind
([CONCURRENT-MIND-AND-GOVERNOR.md](CONCURRENT-MIND-AND-GOVERNOR.md)) and CBAR's analyzer graph.

[Runtime architecture](../README.md#runtime-architecture) · [How a mind runs](HOW-A-MIND-RUNS.md) · [Module architecture](MODULE-ARCHITECTURE.md) ·
[Shared resources](SHARED-RESOURCES.md)

## Summary

The core is made of objects, which this chapter calls **parts**: a speech model, an inference
adapter, a grid transport, a service, a brain region, a persona and each of her parts. Each part
answers three questions, and the core answers each one once.

- **What it is and what it is made of.** A part has an identity, owns its state, and is made of
  parts. It descends from a lineage in which each generation holds only its difference from its
  parent.
- **How a method works on it.** Any method on a part chains to the same method on its parts, then
  adds its own answer. The traversal is written once, so a method is "my parts' answers, plus mine",
  whatever the method is: shutdown, suspend, footprint, health, save.
- **How it lives.** A part runs on its own worker. Typed inputs wake it; it takes a short turn, owns
  what it changes, and publishes what it produced with provenance. Long work becomes a task whose
  completion wakes it again. Nothing calls into a part and waits for it to finish.

Today each of these is answered several ways (see [the problem](#the-problem)). This design keeps
the parts of today's code that are right, names them once, and deletes the rest.

## The problem

Five shapes answer "how does a component live", and none answers all of it:

| Today | Where | What it does |
|---|---|---|
| `ServiceModule` | `continuum-kernel/src/runtime/service_module.rs` | Commands by string name, events by glob, a tick, its own initialize and shutdown; 102 implementors |
| `Daemon` | `continuum-kernel/src/runtime/daemon.rs` | A tick on a cadence publishing a watch snapshot |
| `BrainRegion` | `continuum-kernel/src/runtime/brain_region.rs` | A tick publishing into a ready buffer |
| kernel `Stage` | `continuum-kernel/src/runtime/stage/stage.rs` | The stage contract: triggers, typed topics, input revisions, provenance, a run outcome; used by cognition's deferred faculty |
| flow `Stage` | `continuum-flow/src/stage.rs` | CBAR's `QueueThread`: one input in hand and one pending, its own worker, a QoS class, typed overflow; no other crate uses it |

Three more shapes answer "how does a component start and stop": persona engine's `Lifecycle`
(suspend only), each component root's own `initialize` and `shutdown` (34 `shutdown` bodies), and
the sketch in MODULE-ARCHITECTURE §9.4. And no shape answers "how is a component made of
components": every parent writes its own loop over its children.

The audio models a resident hears and speaks with show the cost.
- **Thirteen backends, one surface, many copies.** The thirteen backends repeat the same surface
  across three traits.
- **The models live in globals.** Each backend's model is a `static`
  (`static WHISPER_RT: ReloadableModel<...>`), not a field the backend owns.
- **Three owners unload them, each its own way:**
  - an idle watcher on a five-second constant nobody measured;
  - a pressure lever;
  - an unload command.

  Each walks the registries in its own serial loop, and none reports a model that is unloaded but
  still held by a transcription in flight. So the memory governor counts bytes as free before they
  are.

Every copy is a place where a fix lands once and is missed everywhere else.

## Goals and non-goals

Goals:

- One object, the part, with one way to be made of parts, one way to chain a method through them,
  and one way to live on a worker and exchange data.
- Each generation of the lineage holds only its difference; each leaf writes only what is new about
  it.
- Results are typed, arrive as they happen, and are never lost: a slow part delays no other, and an
  observer that leaves and comes back sees everything that finished meanwhile.
- State is owned by parts, never by globals.

Non-goals:

- Deciding when to release memory or which work runs first. The resource governor
  ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)) and the flow scheduler (`continuum-flow`) decide.
  This design gives them one surface to drive and exact answers.
- Rewriting every component at once. Each family moves in its own change.

## The design

### 1. A part and its lineage

A part has an identity (its kind: a static name and a sentence; an instance's own id is held by its
owner or is its key among its parent's parts), the state it owns, and its parts. Each generation of
the lineage is a trait whose supertrait is its parent. A behavior shared by several leaves is written
once, at the narrowest ancestor they share.

To find a part's ancestors, read the code as language and ask each word for its parent: a truck is an
automobile; Whisper, Moonshine and Kokoro are each a model, so they share a model generation; three
loops that unload three registries are each an unload, so they are one method. A new trait with the
same verbs as an existing one is a second lineage; extend the existing one instead.

The lineage for the audio models:

```
part                      identity, owned state, its parts
└─ model                  a model it loads and releases (owns its ReloadableModel; no statics)
   ├─ speech to text      transcribe, languages
   ├─ text to speech      synthesize, voices (each resident's own, likely a LoRA voice gene)
   └─ voice activity      detect
```

### 2. Methods chain through the parts

A method on a part is answered by asking its parts the same thing and adding its own answer. This is
true of every method, not only starting and stopping:

```rust
// The traversal is written once. Each method is its parts' answers plus the part's own.
fn footprint(&self) -> Bytes      { self.parts.sum(|part| part.footprint()) + self.own_bytes() }
fn health(&self) -> Health        { self.parts.fold(self.own_health(), |worst, part| worst.min(part.health())) }
async fn shutdown(&self) -> Report { self.parts.each(|part| part.shutdown()).await.then(self.release_own()) }
```

Two kinds of method, because the core is concurrent (section 3):

- **Queries** (footprint, health, identity, what is loaded) read state a part has already published.
  They never wait on the part's worker and never take its lock; the answer is the freshest published
  snapshot, aggregated through the parts.
- **Commands** (prepare, suspend, resume, shutdown, reconfigure) change a part's state. A command is
  a message to the part's worker, and its answer is a completion the caller can await or subscribe
  to. A parent's command sends the same command to each of its parts, then performs its own once
  their completions say it may.

Direction follows dependencies: a part is started before the parts that depend on it and stopped
after them. `prepare` and `resume` run the parent, then its parts; `suspend` and `shutdown` run the
parts, then the parent, and `shutdown` first closes the parent to new work.

A part made of a fixed set of parts (a persona's five) declares them as fields; a part made of a
changing set (a registry of backends) holds them keyed, one part per key, in order. Both are
traversed by the same code.

### 3. How a part lives: its worker and its data flow

Every part runs on its own worker: the flow stage (CBAR's `QueueThread`) that `continuum-flow`
already provides.

- **Wake.** Typed inputs wake it: events (every item matters, read through a cursor) and snapshots
  (only the newest matters, read by revision), as the stage contract defines topics. A command is
  one more input.
- **Turn.** It takes one input at a time and holds one pending. A reliable input (a command, a result)
  is never dropped, and its producer waits for the slot. A signal (a wake, a frame) keeps only the
  newest. The worker's QoS class decides what waits behind what.
- **Short turns.** A turn changes the part's own state and publishes. It holds no lock across I/O and
  never waits on another part. Work that takes longer is dispatched as a task, whose completion is
  one more input that wakes the part again.
- **Publish.** What a part produced is published with its provenance (which run, from which input
  revisions), and everything that depends on it is invalidated and wakes. Nothing polls and nothing
  sweeps on a clock; a cadence is only a floor for a part that must not go silent.

This is what makes command results arrive as they happen. A parent sends `shutdown` to its parts; each
part's worker answers on its own, and each answer is published as it arrives into the parent's
**completion**: an owned record of the command, keyed by part and generation, held until every part
has answered. A slow part holds back no other. An observer reads the completion's current state,
and one that detaches and returns later sees every answer that arrived meanwhile. Pending work keeps
its custody: a model unloaded while a transcription still holds it answers `Pending` on that
transcription's completion, and its bytes stay counted until then.

Replacing a part is a command too. When a key is registered again, the new part takes the old one's
place in the order, and the old part keeps running under its own generation until its own shutdown
completes, held by the completion that owns it. It is a different object with its own worker, so its
release can never touch its replacement.

### 4. The answers a command returns

```rust
pub enum Outcome<P, E> {
    Done,
    Pending(P), // accepted, not finished; P is the completion that will finish it
    Failed(E),
}
```

A part answers a command with an `Outcome`; a part made of parts answers with each part's outcome,
so a caller always sees which parts finished, which are pending and on what, and which failed. A
repeated command joins the one running; a shutdown that arrives during a prepare fences the
prepare's generation so its late completion cannot mark the part ready. No command has a default
answer: a default `Done` would let a part that holds memory claim to have released it. The
generation that knows a part's state supplies its answers once, as the model generation does for
every model.

## Names

One name per concept. Each row's existing names become the new one; nothing keeps two.

| Concept | Name | Replaces |
|---|---|---|
| The object | **part** (`Part`) | the roots of `ServiceModule`, `Daemon`, `BrainRegion`, the speech traits, `AIProviderAdapter`, `GridTransport` as roots |
| What a part is made of | **its parts** | `Children`, `persona_parts!`, `PersonaParts`, every hand-written child loop |
| A part's thread | **worker** (the flow stage) | `Daemon`'s tick task, `BrainRegion`'s tick, `ServiceModule`'s tick loop |
| What wakes a part | **inputs**: events and snapshots, by topic | event globs, string command names, the tick as a trigger |
| What a part produces | **publications**, with provenance | ready buffers, daemon snapshots, ad hoc broadcasts |
| A state-changing method | **command**, answered by a **completion** | `initialize`, `shutdown` returning `Result`, `Lifecycle::suspend` |
| A read-only method | **query**, over published state | getters that lock or recompute |
| A command's answer | **outcome** (`Done`, `Pending`, `Failed`) | `Preservation`, `Result<(), String>` |
| Long work | **task**, whose completion is an input | `Dispatched` runs, spawned futures nobody joins |

`Lifecycle` survives only as the name of the four commands every part answers (prepare, suspend,
resume, shutdown). It is not a mechanism of its own.

## Worked example: a call ends, then memory is needed

1. Two residents finish a call. The voice service is a part whose parts include its speech-to-text
   and text-to-speech registries, each a part whose parts are its models. Whisper and Kokoro are
   loaded; Piper is not.
2. Learning asks for device memory. The governor sends the voice service `shutdown` for its models:
   one command.
3. The voice service sends `shutdown` to both registries, and each sends it to its models. Every
   model's worker answers on its own:
   - Kokoro unloads and answers `Done`.
   - Piper was never loaded and answers `Done`.
   - Whisper unloads, but a transcription that began a moment earlier still holds the model, so it
     answers `Pending` on that transcription.
4. The completion records each answer as it arrives. The governor counts Kokoro's bytes free
   immediately and Whisper's when the transcription completes.
5. A new call starts and sends `prepare`. Whisper's prepare joins nothing stale: the old shutdown's
   generation is fenced, so its late completion cannot unload the model the new call is using.

Today step 3 is a serial loop that logs failures, step 4 does not exist, and step 5 is guarded by a
five-second timer.

## Alternatives considered

- **A lifecycle per family** (an audio `Backend`, an adapter trait). Tried in #6205 and withdrawn: one
  more copy of the problem.
- **Lifecycle as its own mechanism** (a `Verb` enum and a walk that knows only those verbs, #6206's
  first cut). Withdrawn: every other method, footprint or health, would need the same machinery
  again. Traversal belongs to the part; lifecycle is four commands that use it.
- **Calling a part's method directly and awaiting it.** Simple, and wrong for a concurrent core: the
  caller blocks on the slowest part, a dropped caller cancels the work, and results are visible
  only when all finish. Commands to workers with owned completions avoid all three.
- **`&mut self` methods.** Simple for a part with one owner, but parts are shared by handle while they
  run; exclusive access belongs inside the worker's turn, not on the caller's reference.
- **Keeping the five worker shapes and adapting between them.** Each adapter would be one more copy.
  They differ only in which trigger wakes them and what they publish, which are inputs and
  publications of the one worker.

## Migration

Each step deletes more than it adds and lands with its validators.

1. **The part, its parts and the completion**, in `continuum-base::part`. This is a rework of #6206:
   - the traversal is generic for any method;
   - commands go to workers;
   - completions keep each answer as it arrives.

   Persona engine's `Lifecycle`, `Preservation` and `persona_parts!` are deleted; her five parts
   become a fixed set of parts.
2. **The worker.** `continuum-flow`'s stage becomes the part's worker, joined with the kernel stage
   contract's topics, revisions and provenance; the two `Stage` traits become one.
3. **Models.** `ReloadableModel` moves beside the model generation; the thirteen audio backends own
   their models; the statics, their own initialize and shutdown, and the three unload walks are
   deleted.
4. **The other families**, each its own change: `Daemon` and `BrainRegion` (a worker with a cadence
   floor and a snapshot publication), the inference adapters, the grid transports, then
   `ServiceModule`.

## How we will know it works

Validators that run in seconds, with fixture parts and no model or device:
- a part made of a fixed set and one made of a keyed set, driven by the same traversal for a command
  and for a query;
- one slow part and one failing part: the others finish, the failure is named, and an observer sees
  the finished parts before the slow one answers;
- an observer that detaches mid-command and reattaches, seeing every answer that arrived meanwhile,
  with no command reissued;
- a replaced part mid-shutdown, with no other handle to it: it finishes its own shutdown under its
  generation, its replacement untouched;
- concurrent suspend, resume and shutdown on one part, and a shutdown during a prepare;
- an unload while a caller still holds the model: `Pending`, released when the caller finishes.

On the live node, after step 3:
- the bytes the governor counts as released match what the device reports;
- releasing the audio models under pressure no longer waits on the slowest model;
- the five-second idle constant is gone.

## Open questions

- Whether idle release should exist at all, or release is only the governor's decision under
  pressure. The design supports both; the policy belongs in [SHARED-RESOURCES.md](SHARED-RESOURCES.md).
- Where the model generation lives once the inference adapters adopt it: `continuum-base` for now,
  since both `continuum-live` and `continuum-engine` need it.

## Appendix: where the code stands

Survey of canary c819052ef7, 2026-11-20.

| Cluster | What repeats | Owner and target |
|---|---|---|
| How a component lives and stops | Five worker shapes, three lifecycle shapes, 34 `shutdown` bodies, 19 health checks, hand-written loops over children | Fable: this chapter |
| Pressure, footprint, leases | Five overlapping bases; about 150 Lease, Permit, Grant, Gate and Admission types | Codex: the one governor of [SHARED-RESOURCES.md](SHARED-RESOURCES.md) |
| Small utilities | Three backoff formulas, 36 registries, 25 raw `HOME` reads | Each gets one floor owner as it is touched |
| The mind as a tree | `PersonaUnit` and `PersonaCognition` as two unconnected roots | After step 1: her parts, then her regions, as parts |
