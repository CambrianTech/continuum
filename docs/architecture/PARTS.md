# Parts: one lineage and one lifecycle for everything that runs

Status: design, 2026-11-20. Direction approved by Codex; the first cut is in review (#6206).
Owners: Fable (the part tree, lifecycle, model parts); Codex (resources and leases, review).

[Runtime architecture](../README.md#runtime-architecture) · [Module architecture](MODULE-ARCHITECTURE.md) ·
[Shared resources](SHARED-RESOURCES.md)

## Summary

Every long-lived thing in the core (a speech model, an inference adapter, a grid transport, a
service module, one of a persona's parts) starts, pauses, resumes and stops. Today each family does
this its own way, and every parent that owns several of them writes its own loop over them. This
chapter replaces those with one family tree. At the root is `Part`, something with an identity.
Its child generation, `Lifecycle`, adds the four lifecycle verbs with typed reports. Below that,
each generation adds only what differs: a model part adds a model it loads and releases, a speech
model adds transcription. A part made of parts gets its lifecycle by walking its children, and that
walk is written once.

The result we want: adding a new kind of component means writing only what is new about it. Asking
the whole tree to release memory is one call, and it tells us exactly what was released, what is
still in flight, and what failed.

## The problem

Take the audio models a resident hears and speaks with. There are thirteen backends today: four
speech-to-text, five text-to-speech and four voice-activity detectors. A survey of canary
(c819052ef7) shows how they live.

- **Each family declares the same things separately.** `SpeechToText`, `TextToSpeech` and
  `VoiceActivityDetection` each declare a name, a description, an initialize, an is-initialized
  check, parameters and a shutdown. Each of the thirteen backends repeats them.
- **Each model lives in a global.** A backend's model is a `static` (`static WHISPER_RT:
  ReloadableModel<...>` in `stt/whisper.rs`, one per backend), not a field of the backend object.
  Anything may reach it, and nothing owns it.
- **Three owners decide when the models unload**, and each walks the registries by hand:
  - an idle watcher that unloads five seconds after the last call ends, a constant nobody measured
    (`audio/resource_lifecycle.rs`);
  - a pressure lever that calls the same function when the resource authority asks for memory, and
    measures the bytes before and after (`consumers/live_session_consumer.rs`);
  - the voice module's unload command, which contains a second copy of the same two loops
    (`modules/live/voice_module/resources.rs`).
- **The loops lose information.** They release one model at a time, log a failure and move on, and
  report nothing about a model still held by an in-flight transcription.

The audio models are not special. The survey finds the same shape everywhere: six unrelated
component roots (`ServiceModule` with 102 implementors, `AIProviderAdapter`, the speech traits,
`GridTransport`, `Daemon`, `BrainRegion`), 34 separate `shutdown` bodies, 19 health checks, and
parents that loop over their children by hand. The grid loops over its transports in four places.
The only shared lifecycle in the core is persona engine's `Lifecycle`, which covers suspend alone,
and nothing outside that crate uses it.

Every one of these copies is a place where a fix lands in one family and not the others. That is
how the core keeps growing without getting better.

## Goals and non-goals

Goals:

- One lifecycle for every long-lived part, with typed reports that say what is done, what is still
  pending and on what, and what failed.
- One composite: a part made of parts implements its lifecycle by walking its children, never by
  writing a loop.
- Each generation of the tree holds only its difference from its parent. A speech backend writes how
  to load its model and how to transcribe, and nothing else.
- State is owned by objects, never by globals.

Non-goals:

- Deciding when to release. That is the resource governor's job
  ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)). This design gives it one verb to call and an exact
  answer; it does not move the policy.
- Health and footprint. They follow the same tree later, as their own changes.
- Inheritance as Rust lacks it. The lineage is traits, generic owners and composition.

## Design

### A part and its lineage

A part is something that runs and can be named. `Part` has one method, `identity()`, returning the
part's kind: a static name and a one-sentence description ("whisper", "local Whisper inference").
An instance's own id, such as a persona's id or a configured provider's key, is separate. It is
held by the part's owner, or used as the part's key in its parent.

Each generation below `Part` is a trait whose supertrait is its parent. A behavior shared by several
leaves is written once, at the narrowest ancestor they share. To find that ancestor, read the code
as language: list the types and the methods, and ask each one for its parent word. Whisper,
Moonshine and Kokoro are each a model, so they share a model generation. Two loops that release two
registries are each a release, so they share one walk. A new trait with the same verbs as an
existing one is a second lineage, and the existing one should be extended instead.

### The lifecycle

```rust
pub trait Report: Send {
    fn is_done(&self) -> bool;
}

pub enum Outcome<P, E> {
    Done,
    Pending(P), // accepted, not finished; P is the completion that will finish it
    Failed(E),
}

#[async_trait]
pub trait Lifecycle: Part + Send + Sync {
    type Report: Report;
    async fn prepare(&self) -> Self::Report;  // stage for use
    async fn suspend(&self) -> Self::Report;  // quiesce, keeping what an equivalent resume needs
    async fn resume(&self) -> Self::Report;   // bind what suspend kept
    async fn shutdown(&self) -> Self::Report; // release everything
}
```

The decisions, and why:

- **A report, not a `Result`.** Releasing a model can be accepted but not finished: the model is
  unloaded, yet a transcription that started a moment earlier still holds it, so its memory is not
  free. `Result` cannot say that. `Pending` says it and names the completion to wait on. Until that
  completion, the part keeps custody of the bytes, so the governor never counts memory as free
  before it is.
- **A composite's report holds every child's.** A leaf reports one `Outcome`. A composite reports
  one per child, so the caller sees the siblings that finished while another is still pending, and
  sees exactly which child failed.
- **`&self`, with short exclusive transitions.** Parts are shared by handle while they run (a
  registry hands out `Arc<dyn SpeechToText>`), so a verb cannot need exclusive access to the whole
  object. Each verb takes the part's own state lock briefly to register the operation and its
  generation, then releases it. No lock is held across device I/O or across a wait on children.
- **Repeated verbs join; stale ones are fenced.** A shutdown requested twice while running is one
  shutdown. A shutdown that arrives during a prepare fences the prepare, so the prepare's late
  completion cannot mark the part ready.
- **Suspend is a promise.** It quiesces the part's own work and keeps what an equivalent resume
  needs. A part that has nothing to keep reports `Done` at once, but only when that is true.
- **No default bodies.** A default `Done` would let a part that holds memory claim to have released
  it. The generation that knows a part's state supplies the bodies once (see model parts below).

### Composites

A part made of parts implements its lifecycle by walking its children. The walk is written once and
comes in two forms with the same meaning:

- **Fixed children**, such as a persona's five parts (blocks, tokens, genome, learning, decode). The
  `parts!` declaration generates the struct, its identity, its report (the same struct holding each
  part's report) and its walk.
- **Dynamic children**, such as a registry of backends. `Children<K, C>` holds one child per key, in
  registration order.

The walk takes a snapshot of the children and holds no lock while they act. It runs independent
children concurrently, so a slow child holds back no one, and it keeps every child's report,
failures included. Its direction follows dependencies: `prepare` and `resume` run the parent, then
the children; `suspend` and `shutdown` run the children, then the parent, and `shutdown` first
closes the parent to new work.

Replacing a child is where the old code was most fragile. When a key is registered again, the new
child takes the old one's place in the order, and the old child is handed back as `Retired` under
its own generation. It finishes its own release, and because it is a different object, that release
can never touch the child that replaced it.

### Model parts

Most parts that hold memory hold one thing: a loaded model. Their lifecycle is always the same.
Prepare loads the model, suspend and shutdown release it, and resume loads it again. So that
lifecycle is written once, in a generation between `Lifecycle` and the leaves:

```rust
pub trait Loads: Part + Send + Sync {
    type Model: Send + Sync;
    type Failure: Send;
    fn model(&self) -> &ReloadableModel<Self::Model>;     // the field the part owns
    fn load(&self) -> Result<Self::Model, Self::Failure>;  // the leaf's only lifecycle code
}
```

A model part owns its `ReloadableModel` as a field, so the statics go away. Its lifecycle comes from
a generic owner written once:
- prepare and resume call `load` through the container;
- suspend and shutdown unload it;
- an unload while in-flight callers still hold the model reports `Pending` on their completion.

Whether a model is loaded is read from its container, in one place.

A part with no local model, such as a cloud speech session or a test stub, still implements
`Lifecycle`. Its verbs open and close its session; it does not pretend to hold a model.

The kinds keep only their own verbs:
- speech-to-text: transcribe, and the languages it supports;
- text-to-speech: synthesize, and its voices (each resident's own voice, likely a LoRA voice gene);
- voice activity: detect.

### Worked example: a call ends, then memory is needed

1. Two residents finish a call. The voice owner holds the audio registry: one `Children` of model
   parts. Whisper and Kokoro are loaded, and Piper is not.
2. Some time later, learning asks for device memory. The resource governor asks the voice owner to
   release what it can. The owner calls `shutdown` on its registry, once.
3. The walk snapshots the three children and shuts them down concurrently.
   - Kokoro unloads and reports `Done`.
   - Piper was never loaded and reports `Done`.
   - Whisper unloads, but a transcription that began just before still holds the model, so it
     reports `Pending` on that transcription's completion.
4. The registry's report names all three. The governor counts Kokoro's bytes as released at once and
   Whisper's only when the transcription completes. Nothing is reported free before it is, and
   nothing waits for the slowest model to release the others.
5. A new call starts. Each model part's `prepare` loads its model again. A prepare that races with a
   shutdown still in flight is fenced, so the late shutdown cannot unload a model the new call is
   using.

Today, step 3 is a serial loop with logging, step 4 does not exist, and step 5 is guarded by a
five-second timer.

## Alternatives considered

- **A lifecycle per family** (an audio `Backend` trait, an adapter trait, a transport trait). Tried
  in #6205 and withdrawn: it is the existing problem with one more copy.
- **`&mut self` verbs**, as persona engine's `Lifecycle` had. They are simpler for a part with a
  single owner, but a registry hands out shared handles, so `&mut self` would force every shared
  part to sit behind a lock for its whole life. Short transitions on `&self` serve both.
- **`Result<(), E>` for each verb.** It cannot express accepted-but-not-finished, which is exactly
  the case that makes the governor's accounting wrong today.
- **Blanket implementations** (`impl<T: Loads> Lifecycle for T`, and another for composites). Two
  blankets over one trait overlap, and Rust rejects them. Generic owners and the `parts!`
  declaration give the same sharing without the conflict.
- **Default `Done` bodies.** Convenient, and unsafe: a part that holds memory would silently claim
  to have released it.

## Migration

Each step deletes more than it adds and lands with its validators.

1. **The root** (#6206, in review). `Lifecycle`, `Report`, `Outcome`, the walk, `Children` and
   `parts!` land in `continuum-base::part`. Persona engine's `Lifecycle` and its report types are
   deleted, and her five parts become a `parts!` composite.
2. **Model parts.** `ReloadableModel` moves to `continuum-base::part` beside `Loads`.
   - The thirteen audio backends own their models and implement `Loads`, or `Lifecycle` directly
     when they have no model.
   - Their statics, their own initialize and shutdown, and the three registry walks are deleted.
   - The idle watcher, the pressure lever and the unload command each call the registry's one
     `shutdown`.
3. **The inference adapters** onto the same tree, then the grid transports, then `ServiceModule`.
   Each is its own change.

## How we will know it works

Validators that run in seconds, with fixture parts and no model or device:
- a real `Arc<dyn ...>` registry and a fixed composite, both driven through the same walk;
- one slow child and one failing child: the others finish, the failure is named, the finished
  siblings are visible meanwhile;
- concurrent suspend, resume and shutdown on one part;
- a shutdown during a prepare;
- replacing a child mid-release;
- an unload while an in-flight caller holds the model.

The first cut covers the walk, the composite and replacement.

On the live node, after step 2:
- the memory the governor counts as released matches what the device reports released;
- releasing the audio models under pressure no longer waits on the slowest model;
- the idle timer's constant is gone, replaced by the governor's own decision.

## Open questions

- Should the idle release exist at all, or is release purely the governor's decision under
  pressure, with models kept warm otherwise? The design supports both. The policy belongs in
  [SHARED-RESOURCES.md](SHARED-RESOURCES.md).
- Where the model generation lives once the inference adapters adopt it. It is in `continuum-base`
  for now, because both `continuum-live` and `continuum-engine` need it.

## Appendix: where the code stands

Survey of canary c819052ef7, 2026-11-20. Ranked by how much logic each cluster repeats per leaf.

| Cluster | What repeats | Owner and target |
|---|---|---|
| Component lifecycle and health | Six roots, 34 `shutdown` bodies, 19 health checks, hand-written child loops | Fable: this chapter |
| Pressure, footprint, leases | Five overlapping bases and about 150 Lease, Permit, Grant, Gate and Admission types | Codex: the one governor of [SHARED-RESOURCES.md](SHARED-RESOURCES.md) |
| Small utilities | Three backoff formulas, monitors bypassing `Daemon`, 36 registries, 25 raw `HOME` reads | Each gets one floor owner as it is touched |
| The mind as a tree | `PersonaUnit` and `PersonaCognition` as two unconnected roots | Follows step 1: her parts, then her regions, on this tree |
