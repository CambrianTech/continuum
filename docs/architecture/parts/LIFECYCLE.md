# Lifecycle

Status: design approved in direction by Codex, 2026-11-20; landing in `continuum-base::part`.
Replaces three shapes: persona engine's `Lifecycle` (suspend only,
`continuum-persona-engine/src/persona/lifecycle.rs`), each component root's own `initialize` and
`shutdown`, and the trait sketched in MODULE-ARCHITECTURE §9.4.

[Parts](README.md)

## The trait

```rust
/// What a verb reports. A leaf reports one outcome; a composite reports one per child.
pub trait Report: Send {
    fn is_done(&self) -> bool;
}

pub enum Outcome<P, E> {
    Done,
    Pending(P),   // accepted, not finished; P is the completion that will finish it
    Failed(E),
}

#[async_trait]
pub trait Lifecycle: Part + Send + Sync {
    type Report: Report;

    async fn prepare(&self) -> Self::Report;  // stage for use
    async fn suspend(&self) -> Self::Report;  // quiesce, keep what an equivalent resume needs
    async fn resume(&self) -> Self::Report;   // bind what was kept
    async fn shutdown(&self) -> Self::Report; // release everything, safely
}
```

`Outcome` is a `Report` (done when `Done`). Persona engine's `Preservation` and `Suspension` are
this pair, renamed and generalized from suspend to all four verbs.

## The rules every part keeps

- **Shared handle, exclusive transitions.** Verbs take `&self`, so a part held as `Arc<dyn ...>` can
  be driven. Each transition takes the part's own state briefly, registers the operation and its
  generation, and releases it. No guard is held across device I/O or a wait on children.
- **Idempotent, and joined.** A verb repeated while it is running joins that operation; repeated after
  it finished, it reports the same result. Shutting down a part that never prepared is `Done`.
- **Fenced.** A shutdown during a prepare fences the prepare's generation, so its late completion
  cannot mark the part ready.
- **Suspend is a promise.** It quiesces the part's own work, keeps what an equivalent resume needs,
  and keeps custody of its bytes until the release is acknowledged. A stateless part reports `Done`
  at once only when that promise is actually met.
- **Pending names its completion.** `Pending(P)` carries the owned completion that advances the
  report (an in-flight caller's handle, a device fence), never a bare "later".
- **No default bodies.** A generation that knows its state supplies the bodies once for its
  descendants ([model parts](MODELS.md)); `Done` is never a default.

## Composites

A part made of parts implements its verbs by walking its children. The walk is written once in
`part`, in two forms with one meaning:

- **Fixed children:** a struct of named parts, such as her five parts. The `parts!` declaration
  (today `persona_parts!`) generates the struct, the walk and a report with one field per part.
- **Dynamic children:** `Children<K, C>`, an ordered set holding one child per key. The report holds
  one outcome per key.

The walk:

- snapshots the children and holds no lock across any child's verb;
- runs independent children concurrently, so a slow child never holds back the others;
- returns the full report every time: finished siblings are visible while others are pending, and a
  failure is named, never dropped;
- follows dependencies: `prepare` and `resume` run self, then children; `suspend` and `shutdown` run
  children, then self. `shutdown` first closes the composite to new work.

**Replacing a child.** Registering a key that is present keeps the key's place in the order and
retires the old child under its own generation: it finishes its own release, and it can never release
or mark the child that replaced it.

## What stays narrower

Selection policy (which child is active, init preference), parameters, and a kind's own verbs
(transcribe, synthesize, detect) stay with the kind's owner. A configured instance id (a provider key)
is the composite's key, distinct from the static kind name in `Identity`.

## Validators

Seconds each, with fixture parts and no model or device:

- a real `Arc<dyn ...>` registry and a fixed-field composite, both through the same walk;
- one slow child and one failing child: the others finish, the failure is named, the slow child's
  outcome arrives, and the report shows the finished siblings meanwhile;
- concurrent suspend, resume and shutdown on one part: each joins or fences as above;
- shutdown during prepare: the prepare's late completion does not mark the part ready;
- replacing a child mid-release: the old child finishes its release, the new child is untouched;
- repeated registration of one key: one child, its order kept;
- an unloaded model still held by an in-flight caller: `Pending` on that caller's completion.
