# Threads on SAGE: one shape for every concern that works on a shared capacity

Status: design for review (Codex) before code, 2026-11-27. Owner: Fable.

Joel, 2026-11-27: "need far more formalized architecture like rockstar and consistent across concerns so it's
unified and easy for top level or even the same sage components to manage... the clean efficient forms
materialize by the problems you've found, repetitive patterns: choose what looks best."

This chapter names the form that the last week's problems kept asking for. It adds one parent, `Thread`, beside
the `Resident` SAGE already has, and states the rules both obey. Everything else here is an instance or a deletion.

## What kept going wrong (receipts)

Every one is the same defect: a concern invented its own unit of work, its own "doesn't fit" answer and its own
modes, so the controller above it could not see or shape it.

| Concern | What it did | What it cost |
|---|---|---|
| Learning a reply | five entries (plain chunk step, whole graph, explicit tile, automatic tile, scattered refusals) | the plain split was a valid mode, so it went unseen: every live reply past `n_ubatch` (2048 on the M5) learned a partial gradient, cosine 0.960 / norm 0.944 on Qwen3.6-35B-A3B, 0.949 / 0.889 on Qwen3.5-0.8B (fork 1eb66d1db, M5 Metal, 2026-11-27); a patch adding a sixth branch then broke the whole-graph caller (Codex on 398bf318d) |
| A tile that does not fit | `fail("learn this reply at a smaller tile")` | a step lost; the caller re-plans with no feedback |
| A unit longer than its tile | prefix decode and stride replay ran whole | a slice could pass its bound unseen; fixed by pending spans (fork fa85c3379), one concern at a time |
| Cost of a unit | each reported its own way, or not at all | `llama_opt_learn_last_unit` was added after the fact (fork 1eb66d1db); `prefill_rate::observe` and `Moved` are two more private shapes |
| Room for the work | at least ten lease and grant types (`GrantLease`, `KvSlotLease`, `HoldLease`, `CoveringLease`, `HostCacheLeaseInputs`, `StickyLease`, `LaneLeaseTerms`, `NodeGrant`, `RegisteredGrant`, `GrantRefusal`, `ThroughputJob`) | each admission decides alone whether it can run |
| Priority | `slice::weight` fixed at 1, 2, 4 | shares nobody set from feedback |
| Device work landing | `Transfer` dropped unlanded read freed memory (fixed by settle-on-drop) | a completion was each mover's own concern |

## The form: a holder is a Resident in space and a Thread in time

SAGE already owns space: `place::Resident` reports its rows (held, floor, want, how it varies) and implements only
the bodies of its ways (resize, cache out, collapse); the parent does the rest once (SAGE.md, "Intermediate parents").
`Thread` is the same contract in time. A concern that does work on a shared capacity (the Core, a link, the flash,
a CPU worker) is a Thread; what it keeps between its units is a Resident.

```rust
/// What a thread owes and how it can be cut, said as state on every change (latest-wins, never events).
pub struct Owed<U> {
    pub class: Class,          // Threshold { bound } (a resident's act, a frame) or Share (learning, a dream, a prefetch)
    pub left: U,               // work remaining, in its own quantity: rows, tokens, bytes
    pub lever: Lever<U>,       // the unit sizes it can run: least ..= most, and its grain
    pub since: Instant,        // the oldest work waiting, for aging
}

/// What one unit of a given size needs beside it, from the holder's own measure (bytes are known, not learned).
pub struct Needs<P, Q> { pub claims: Rows<P, Q> /* scratch per place, held only while the unit runs */ }

/// What the controller hands a thread: a unit size it chose and the claim prepared for it.
pub struct Slice<P, Q, U> { pub unit: U, pub claim: Claim<P, Q>, pub slack: Duration }

/// What one unit did, measured. The record every controller learns from.
pub struct Ran<U> {
    pub did: U,                // work advanced (rows decoded and trained, bytes moved)
    pub lasting: Lasting,      // useful work that lasts: a committed update, a turn's tokens within its bound, a block landed before its use
    pub next: Next,            // More, Done, or Pending { fence } while the device finishes
}

pub trait Thread<P, Q> {
    type Unit: Copy + Ord;
    /// Required: what it owes and how it can be cut. A holder that cannot say this is not a Thread.
    fn owed(&self, into: &mut Owed<Self::Unit>);
    /// Required: what a unit of this size needs. Pure; never runs anything.
    fn needs(&self, unit: Self::Unit, into: &mut Needs<P, Q>);
    /// Required: run exactly one unit inside its slice. Never more than `slice.unit`, never a refusal for size.
    fn run(&mut self, slice: Slice<P, Q, Self::Unit>) -> Ran<Self::Unit>;
}
```

The parent, written once, owns everything a leaf used to reinvent:
- publishing `Owed` as state on change (today's `learning/owed` becomes its first instance);
- the time model: each `Ran`'s measured duration fitted per unit size (`decide::Measured`), with SAGE's algorithm
  lifecycle (default, resume, learn, checkpoint), so nobody hand-writes `prefill_rate` again;
- the fence: a `Pending` unit is settled by its `Completion` (the block layer's, `continuum-blocks/src/link.rs`)
  before its claim is released, so no mover owns landing twice;
- stamping every slice and `Ran` into the decide replay log;
- splitting in place: a unit whose claim cannot be prepared is offered at the next smaller size in its lever; at
  `least`, it waits in its class (time divides). The leaf never sees a "too big".

## The controller: one loop, the lane

Per step, for each capacity (the Core first; a link and the flash are the same loop):
1. Read every thread's `Owed` (state reads, never awaits).
2. `decide` chooses which thread runs and its unit size, inside the envelope: the unit within its lever, its
   predicted time within the slack every threshold thread has left. Shares per class are this decision's output
   from feedback (slack against the bound for a threshold class, queue growth for a share class), never constants.
3. The placer prepares the unit's claims (`place`): granted, or a wait naming its shortfall, which is the next
   decision's input as a smaller unit. Never a refusal for pressure.
4. The thread runs one unit; the `Ran` goes to the slicer (`Stepped`), the decide record, the time model, and the
   thread's next `Owed`.

The objective is power: lasting work per second, each class under its own contract. A transfer is time spent
moving; a refused or discarded unit is time spent for no work, so it is never among the controller's actions.

## The rules (every thread, every review)

1. **One path per concern.** A special case is a unit size, never a mode: a whole-reply graph is one tile with
   `unit >= left`; a single-token decode is a unit of one.
2. **A unit is bounded and declared.** `run` does at most `slice.unit`, and `Ran.did` says what it did; the parent
   asserts `did <= unit` (the fork's tiled-unit gate is this rule's first test).
3. **No refusal for size or pressure.** Room is space or time: a smaller unit or a later slice. The lever's `least`
   is the only floor.
4. **Only "never here" is typed, and it is decided where the plan is chosen.** A layout a thread cannot run on is
   made unrepresentable by the type it takes (the walk takes her own stream, so a shared cache is not passable),
   or refused once when the thread is seated, before any unit.
5. **Context is a Resident; scratch is the unit's claim.** What lives between units (checkpoints, accumulators, KV,
   an optimizer) is reported and paged like any holding; graph workspace exists only inside a claim.
6. **Owed is state, latest-wins.** A thread leaving the map is its Done; a dropped say can never strand a Ready.
7. **Useful is what lasts.** No controller is rewarded per unit advanced, so cutting the same work finer never
   looks like more power (Codex, 2026-11-26).
8. **Exactness is checked against itself and one other backend.** A thread whose result must not depend on its unit
   size (learning) is gated by `unit = left` against `unit = k` on the same backend, at that model's measured floor
   (the same configuration twice), plus one cross-backend reference. A fixed threshold against a graph built from
   different kernels can judge the reference instead of the thread (CUDA's 0.498 on the 0.8B: its tiled forward
   loss matches CPU's within 3.2e-5 while its whole-reply reference is 0.004 off; Cormac's inference that the
   reference is the outlier is being checked by a layer bisect, 2026-11-27).

## The instances

| Thread | Unit | Lasting | Context (Resident) | What it deletes |
|---|---|---|---|---|
| Mind (decode) | one step: a token for her, or a batch of seated minds | her turn's tokens within the act bound | her KV and recurrent state | the turn-long hold (`frame.rs`, already going) |
| Prefill | rows, up to `n_ubatch` | rows cached in her window | her KV | `driver/prefill_rate.rs` as a private fit |
| Learner | tile rows | a committed optimizer update | checkpoints, the K/V-gradient accumulator, the cursor, the optimizer | the fork's plain chunk step, `graph_step`, `opt_learn_tile`, `opt_tiles_admit`/`opt_tiles_past_ubatch` (398bf318d), "tile did not fit", the host-budget refusal; the driver's whole-reply path, `measure_defect`, a seat `Refused` for room |
| Streamer (a link) | one block | a block landed before its use | the blocks in flight (`InFlight`) | the Streamer's own feed/land pacing |
| Checkpoint / save | bytes | bytes durable | the staged copy | private write loops |

The lane's key is already generic (`Slicer<K>`); it becomes `Thread { Mind(PersonaId), Learner(SeatId), Prefill(PersonaId) }`
on the Core and a `BlockKey` on each link. Each lease type in the table above folds into `place::Claim` when its
caller becomes a Thread, and is deleted then.

The fork's C surface takes the same shape, so the driver adapter is only the diff:
`llama_opt_learn_begin(reply)`, `llama_opt_learn_needs(ctx, rows) -> { device, host }`,
`llama_opt_learn_unit(ctx, rows) -> { decoded, trained, more }`.

## The order: one PR each, stacked, each deleting more than it adds

1. **SAGE `Thread` parent**: the types above, the parent's state, fit, fence and split; the slicer keyed by thread.
   Validator in seconds: `slice::sim` with a threshold class and a share class; the share's queue stays in check
   while no threshold thread passes its bound, and a unit too large for its claim is run smaller, never failed.
2. **The fork's learn collapses** to begin / needs / unit. Deletes the five entries; `test-walk-exact` rewritten to
   rule 8. Gate: 0.8B on Metal, CPU and CUDA; the 35B on Metal (the M5's production form); CUDA's reference
   question settled first.
3. **The Learner thread** in the engine over it; `learning/owed` becomes its `Owed`. Deletes the driver's whole-reply
   path and every learning refusal for room. Gate: a loaded window on the M5 with the 35B, the queue in check, no
   resident past her bound, every Ran recorded.
4. **Prefill and decode** as instances (the Mind thread exists; its unit and `Ran` become the parent's).
5. **The Streamer** as a thread on each link; `Transfer`'s completion becomes the parent's fence.
6. **The lease types** fold into `Claim` as their callers move.

## Questions for Codex

1. Is `Needs` (space, from the holder's measure) and the parent's time fit the right split, or should a leaf
   also predict its time (the fork can estimate a tile's work exactly from rows)?
2. Batched decode makes one unit serve several Mind threads. Is that a Thread over the batch, or a unit the
   controller composes from several threads' slices?
3. Rule 4 on CUDA and Windows: is every serving context a stream per mind today, so the walk's own-stream type
   costs nothing there?
