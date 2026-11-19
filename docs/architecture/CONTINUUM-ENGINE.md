# Continuum Engine

**Status:** the design of the engine that serves our minds, as the code on `canary` implements it,
with each proposed change marked **Proposed** where it applies. Edit this chapter like code: change
a section in place when its object changes, and delete what is superseded. History belongs in
commits and PRs, and receipts belong in the PR that made them. The runtime contract the engine is
built on is [SUBSTRATE.md](SUBSTRATE.md). Stage types are in [STAGE-CONTRACT.md](STAGE-CONTRACT.md),
budgets and residency classes in [SHARED-RESOURCES.md](SHARED-RESOURCES.md), training in
[IN-PROCESS-LEARNING.md](IN-PROCESS-LEARNING.md), and where each module runs in
[MODULE-ARCHITECTURE.md](MODULE-ARCHITECTURE.md) §9. Order of work and owners are in `CLAUDE.md`
and on the cards, never here.

## 1. What the engine is

Many persistent minds share one live model the way a game's renderer shares one GPU. The weights
load once and the engine stays up. **One mind decodes at a time at the engine's full rate**, while
the next is staged as data and the previous retires to RAM, and a switch binds her blocks in
milliseconds. Four minds decoding together on the M5 ran at about 2 tok/s each (8 in total); one
at a time runs at the full rate. Every byte stays native, every reuse is taken, and nothing waits
that need not.

**Gates** (a change to the engine reports both, before and after, on the live node):

| Gate | Target | Measured by |
|---|---|---|
| Delivered tokens per second over 900 s of real resident work, swaps and waits included | at least 60 on the M5, 200 on BigMama, with per-resident progress reported | `Record` turn rows (§9) |
| Engine occupancy while a mind is resident | at least 800 permille busy, no idle gap over 2,000 ms | `EngineSnapshot` (§9) |
| Prefill per act | equal to the appended tokens (§6) | `Prefill` decision probe |
| Switch on the critical path | milliseconds, never a serialize to disk | the tier's `Rebind` report |

Two rates, never confused: the **wall-window rate** is output tokens delivered to minds over the
whole window, divided by its wall time (prefill, switches, waits and idle all count against it).
The gate is this rate. The **decode-only rate** is output tokens divided by time spent in decode
steps, and it states the engine's speed for one mind at her depth: 26-32 tok/s on the M5 without
the MTP draft. So 60 needs the draft and prefill overlapped with decode (§3), not one mind alone.

## 2. Objects and ownership

The engine follows the [substrate extension contract](SUBSTRATE.md#extension-bar).
For example, `PersonaEngine` checks admission with `Roles` before asking a `Tier` to
begin copy-in. Each tier implements the native transfer. Every tier inherits the same
admission order through the engine entry point.

**Proposed adapter catalog.** The registry selects providers and snapshots their handles
in priority order. Release its read guard before querying them. A provider that supports
listing implements a required catalog read against its loaded adapters. Distinguish an
unsupported provider, a failed read and a successful empty list in the query result.
Diagnostics and residency consume that observation. The decode owner reports which
adapters and scales it applied to an individual turn.

The table identifies the owner of each operation and its state.

| Object | Where | Owns |
|---|---|---|
| `PersonaEngine` | `continuum-persona-engine/src/engine.rs` | the engine's one decode: `submit` (a step of the active mind, after the previous step landed), `stage` (begin the next mind's copy-in), `handoff` (refused until the in-flight decode has landed) |
| `Roles` | `pipeline/roles.rs` | which mind is active, next and retiring, and the transitions between them. Exactly one mind is active, by construction. A refusal returns every owned value it received |
| `PersonaUnit` | `persona/unit.rs` | her five parts as one value (blocks, token history, basis, learning, decode marks). The only constructor takes every part, so a move is whole by type |
| `MindId { persona, epoch }` | `persona/id.rs` | her identity. The fork's 64-bit stamp is derived from it inside the tier only |
| `Tier` | `residency/tier.rs` | the native moves for a mind's blocks (bind, save, checkpoint, rewind, write, read). One contract for VRAM, pinned RAM, the unified pool and flash. It keeps only which stream holds which mind |
| `Fence` | `residency/fence.rs` | a transfer's completion, tied to the mind and direction it moves. Readiness is never visible before its fence lands |
| `Execute` | `residency/execute.rs` | the decode a tier runs for the mind bound to its stream. It takes her tokens, never a sequence id, so no step can write another mind's state |
| `Footprint`, `Lifecycle` | `persona/footprint.rs`, `persona/lifecycle.rs` | what a node holds is the sum of its children, and each owner suspends its own children and returns their results |
| `protocol` | `protocol.rs` | the one seam between minds and the engine: typed commands in, typed events out, one published snapshot (§3) |
| `Scheduler` and its driver | `continuum-engine/.../backends/persona_scheduler.rs`, `driver.rs` | the in-process engine over one libllama context: the frame loop (§3) |
| `EngineGeometry` | `persona_scheduler/geometry.rs` | what the context actually applied (window, batch, sequence layout), named by engine incarnation (§7) |
| `PreservedMinds` | `persona_scheduler/preserve.rs` | what outlives a restart (§5.3) |
| `Emit`, `Record` | `persona_scheduler/emit.rs`, `record.rs` | an answer's text and stops; every measurement of a step and a turn (§3) |

**The grid is one more tier.** A mind is a value her blocks travel with, so moving her to another
node is a `Tier` move whose transfer crosses the network. Its fence lands when the peer holds her
blocks, and a refusal returns her whole. **Proposed:** before the tier's shape is fixed, a replay
covers a lost acknowledgement: the peer accepts her unit, its ACK is lost, the sender retries, and
the old completion arrives after the stream was reused. The two nodes must agree on one owner of
her decode, and a byte stream received alone never grants a second decode authority. The transfer's
identity and its retry replay belong to this tier. What the transfer costs, and who is charged for
it, is accounted in [SHARED-RESOURCES.md](SHARED-RESOURCES.md).

**Proposed: the rest of the object map.** Each row deletes the copies it names, in this order:
1. `EngineConfig` resolved once from what the engine reports; every context is built from it (§7).
2. `Answer` and `Emit` own an answer end to end. The backend's collector, its sleep-spin and its
   `spawn_blocking` per turn go, and media joins the one decode path as an `Extend` (§3).
3. `Record` derives every turn time from one span per step. The other five measurements go.
4. The engine becomes a kernel `Lifecycle` node whose children are its minds, with
   `PreservedMinds` over the saved-state `ExternalBlob` medium.
5. The prefill and scheduling policy each have one owner. `turn::plan` and `NextPolicy` go.
6. Delete what this replaces: lanes, `kv_page`, served windows and the media bypass, split by crate
   (serving, grid and governor: Codex; engine planning, persona and spawner: Fable).

## 3. The frame loop

The engine is an object you submit to, and its thread is a frame loop. It never serves a whole turn
in one call. Each frame advances one turn by one step, either one prefill chunk or one decoded token,
so no mind waits behind another's whole prompt (`persona_scheduler/frame.rs`). Before this, turns
served whole waited 300-360 s behind each other's prefills.

**Which turn the next frame advances:**
1. The current scheduler continues an active decode until the answer ends. On the 5090 HTTP
   path, a single endpoint permit also covers the whole generation. An 8,126-token answer
   occupied one resident slot while another resident with its own slot still awaited admission.
2. For prefill, select the least un-reusable work remaining, discounted by time already queued.

**Required handoff under contention.** Keep decoding while no other ready turn needs service.
When a foreground turn becomes ready, the scheduler compares its time to first output with the
cost of interrupting the current stream, including both transfers and the next audio deadline
when speaking. Stage the selected successor while the current step runs. At a landed step,
the existing residency owner can hand off the stream. The interrupted turn retains its
sampler, token history, stop matcher and pending output in order. Its next frame resumes
from the landed token. Measure the longest gap in each active stream and the oldest ready
turn as well as total tokens per second. This policy requires resumable decode on the
serving backend. Releasing an HTTP admission permit while its backend still generates
would misrepresent available capacity.

Prompt assembly owns interest in the next swap. A capacity or slot-release event makes
that work runnable while assembly proceeds. A pending swap holds no mind or slot and
needs no timer. When assembly finishes, the foreground request takes responsibility for
the same page. If the transfer has already begun, its existing completion owner retains
physical custody until the native receipt lands. If the turn is cancelled before any
transfer, dropping its interest withdraws the pending work.

**Nothing on the engine thread waits.** Everything else runs as a `continuum-flow` stage, CBAR's
`QueueThread<T>` (SUBSTRATE §2): its own worker, a bounded inbox, woken only by a push.
- **Emit** turns tokens into text on its own worker, streams them to the caller, and watches for
  stop sequences. A stop it sees, or a caller that hung up, raises the answer's cancel flag, which the
  engine reads at its next step without a lock.
- **Record** merges each step's span into busy windows and publishes the `EngineSnapshot`
  (occupancy and longest idle gap), and writes each turn's probe row.
- Each stage's `StageWorker` is its one lifecycle owner (`stop`, then `join`). The driver joins Emit
  and Record after it exits.

**The protocol** (`protocol.rs`). A mind runs many activities (a card, a conversation, a review, her
own reflection), and her identity and memory are the prefix they share. A command is sent and the
sender goes on. Its outcome is an event that the interested stage reacts to. A tool's result is an
`Extend` into the activity that called it, whenever it arrives. Commands to one activity apply in
the order sent, and different activities interleave as the scheduler decides.

**A lock only guards an enqueue.** A permit or lock held across IO, compute or another wait
serializes everything behind it. Work moves through stage inputs and fences, never under a held
permit.

**Proposed: overlap.** The next mind is copied in, and her prefill chunks run, during the active
mind's sample steps. Gate: switch time on the critical path near zero, and GPU idle between the
active mind's tokens near zero.

## 4. Admission

A mind is never refused for lack of a fixed share. Admission has one owner per path, and every wait
is an event.

**The invariants** (cited in code by label):
- **I1. A stop saves every resident, and never ends a live engine it did not save.** Every endpoint
  goes through one checkpoint owner, whose intent (stop or replacement) is explicit. The checkpoint
  returns typed evidence per resident (saved, refused or uncertain) to the authority that may end the
  engine. A failed or uncertain save keeps custody and refuses the destructive stop. A resident is
  named lost only after the engine's death is verified, or a destructive stop was explicitly
  authorized (§8).
- **I2. A restart restores her newest state.** For unchanged input her first act reuses the longest
  matching prefix. When her input differs (her view turned over, or her adapter or the model changed),
  the act carries the typed invalidation cause, never a silent `cached=0`.
- **I3. A ready mind is dispatched at once.** There is no queue position, grant state or tick. At her
  act she takes free cells. If short, the coolest holders not mid-act are paged out. Only when every
  holder is mid-act does she wait, woken by an act's end. Every other no-room case is a typed outcome
  with its own event.
- **I4. A claim is held for an act, and KV occupies its cells until its erase is verified.** Between
  acts her KV is cache, reclaimable by temperature. Dropping a logical lease never frees physical
  cells.
- **I5. Every wait is an event.** Grants are evaluated when demand moves (a ready mind, an act's end,
  a pool step), never on a tick.

**The policies, each with its invariant:**
- **P3, grants at the act** (I3, I4): `ActHold`, reservations and sealed `GrantHandoff` receipts.
  Standing grants, waiter queues and head-of-line rules are deleted.
- **P4, a restart is warm** (I2), for a restart in the same format: adoption keeps a slot's live KV
  resident, and a restored page that reuses nothing is found by its token-level divergence. A first
  boot that crosses formats (llama-server page files into the persona engine) has no reader yet.
  **Proposed:** a typed importer converts each mind's page file (tokens and state in one file, with
  its `.lora`, `.ckpt` and `.dft` companions and the saved ledger naming her) into her preserved
  unit, and refuses her by name when her epoch, adapter or sentinel cannot be proven. It never
  reconstructs her history from chat text. **P4a**, genes change in place through
  their one owner: a load is plan, lease, register the residency, then load inside it, and an unload
  carries the engine identity captured before the catalog read, so a replaced engine refuses. The
  genome side is IN-PROCESS-LEARNING.md.
- **P5, the scheduler**: memory follows the acting minds, and no mind holds a standing share. On the
  in-process engine, decode concurrency is one mind by construction (`Roles`). The capture is the
  workload, not only the chosen steps: arrivals, readiness, page moves and each step, so a replay can
  score another policy on the same arrivals. A learned policy replaces the measured baseline only by
  beating it on that replay.
- **P6, no swap**: KV, host cache and checkpoints share one measured budget with the OS's pressure
  verdict. A shedder reports what it can give back before it is asked (**G4**: one answered
  `before=365202 after=365202` 460 times in 47 minutes).

**Decode rank on the external-driver path.** `turn_admission` (llama-server, used by BigMama and the
learner) ranks each claim by its request class (`SlotClass::decode_rank`): foreground for turn, fork
and sidecar; background for background, dream and probe. `DecodeBound::try_claim` and `claim` make
the one priority decision. A returned permit goes to a waiting foreground claim before any
background one. `DecodeBound::with_permits` builds an adapter's bound once, and every clone of it
shares the one rank. **Proposed:** a typed `AdmissionRequest` (a paged turn versus an unpaged one)
replaces deriving the rank from an optional page key. That path then converges onto the frame loop
(§3) and is deleted.

## 5. Residency

A mind's blocks live in one of the tiers: bound to a stream in VRAM, warm in RAM, or cold on disk.
**Nothing computed is discarded**: a restart or an eviction demotes her a tier, and never drops her.

### 5.1 Native blocks, bound like descriptors

Her blocks stay in the engine's native layout. A switch binds them, the way a renderer binds a
descriptor set, and never walks or re-serializes them. The fork's `llama_persona_*` and
`llama_state_seq_*` calls are the moves, wrapped by the `Tier` adapter. The pipeline decides when
they run, and never branches on which tier it has.

### 5.2 Fenced moves

Staging the next mind and retiring the previous one are transfers through the tier, each under its
fence, and they proceed while a decode runs. `handoff` binds the next mind once her fence lands, and
leaves the previous mind awaiting copy-out. No read of her blocks starts before she stops being
active. `release_retired` returns her to her tier once the copy-out lands.

### 5.3 Restore is the norm

A stop writes every mind's blocks, and a start restores them. `PreservedMinds` is a kind of the one
saved-state contract (`continuum-saved-state`): an atomic save, a generation and digest, and a typed
load verdict. The contract compares the model field by field, so another model's minds load as
`Diverged`, never as hers. The fork writes each mind's blocks beside the record, and checks owner,
epoch, memory layout and every row length on read. Each stop writes one complete generation, as
`<persona>.<generation>.cpb`, never over a file an earlier generation still names. **Proposed:** the
blocks move to saved-state's `ExternalBlob` medium, recorded by receipt. Gate: no cold first act
after a deploy.

## 6. A mind's context

An engine reuses a prompt up to the first token that differs from the previous one, and recomputes
everything after it. **A context is a FIFO**, and every rule follows from where a change lands:
1. **Order by stability.** An immutable head (identity, tools, policy), an append-only middle (her
   history and results), then a tail that alone may change (this act's facts, the clock, the newest
   result).
2. **Never re-place what exists.** An item already sent stays where it was sent. Moving, reordering
   or rewording it re-sends everything behind it.
3. **Page by position.** The head is one copy that every mind shares. The middle is cold and pages
   to RAM and disk. The tail is hot and stays in VRAM.
4. **Collapse only the oldest end, and only at a restart.** Never clip her newest result: it is the
   step that ends her recursion, and without it her next input can equal her last, so she repeats
   the act.
5. **One health number:** re-processed tokens per act against appended tokens. Healthy is about 1:1.
   At 12:1 four residents saturate the engine. At 1:1 the same machine seats many minds.

**One codec.** Everything a mind perceives goes through `continuum-context`: keyed items that only
append within an epoch, levels of detail generated from each item's shape, and admission instead of
turnovers. A new layer implements `Source` and gets caching, levels and ordering for free. The
source-hygiene ratchets `prompt_messages_outside_the_view`, `collapse_outside_the_view` and
`hand_churn_labels` refuse a prompt message pushed by hand.

**Her token history is the engine's.** A turn whose prompt extends her history under the same basis
prefills only the new tail (`turn.rs`). Anything else (another model or adapter set, another epoch,
other media behind the same ids, a rewritten conversation) starts a new epoch, so stale blocks are
never decoded against. The hybrid model's recurrent state can only go back to a checkpoint, so the
furthest legal start is her checkpoint at her last view's end (`reuse.rs`). Before this, a changed
trailer re-prefilled her whole prompt on every act.

**Proposed: marks.** `extend` renders from her last mark, and a mark names the reuse boundary that
`reuse::common_prefix` finds today by comparison. Tokenizing and the reuse decision move off the
engine thread. Before that: if the chat template rewrites past turns (Qwen's templates are reported
to drop earlier turns' reasoning; to verify against the GGUF's own template), a probe counts every
append whose render differs before its mark, so a rewrite is never silent.

## 7. Engine configuration

Engine configuration was decided in five places, with defaults between them. That is how the GPU
matrix units stayed off, how a hung device probe moved a 27B to the CPU, and how an explicit
`--parallel` silently turned off the shared KV pool.

- **What the engine applied is read from the context it created**, never from what was asked
  (`EngineGeometry`). Each snapshot names its engine incarnation, so a reader retires a stale
  observation instead of acting on an engine that is gone.
- **Proposed: one `EngineConfig`**, resolved once under `inference/planning` from what the engine
  reports about the device (through a query, never a parsed log or a chip name) and the measured
  memory budget. It covers every decision a context is made with: compute operations and data types,
  placement of the base and the draft, KV layout, and checkpoint policy. The geometry is checked
  against it, and a mismatch refuses with its cause, with no fallback. It replaces
  `SchedulerConfig`, `LlamaCppConfig`'s engine fields, `LlamaCppLaunch` and the adapter's overrides,
  and their three contradicting values for flash attention, fused GDN and sequence count.
- **Proposed restore contract.** Before context allocation, the preservation owner supplies
  the native layout and capacity each saved mind requires. The native reader defines those
  requirements. A token history length alone cannot establish that its block file fits.
  The turn owner supplies prompt and output needs. The tier supplies the streams that can
  coexist. The planner budgets the active and staged streams together and accounts separately
  for parked blocks in backing storage. Only the active stream decodes.
  Before publishing readiness, check the applied geometry against these requirements. Report
  each refused mind and its limiting constraint, and retain its last durable generation for
  retry. Serving and governors read the same engine incarnation's per-sequence window and
  admission decision. Neither derives a resident's allowance from total context and a local divisor.
- **Launch flags carry startup facts only.** A context resizes live, and a relaunch happens only for
  a dead engine.

## 8. Custody and stop

A stop is one owner deciding, per endpoint, what it holds. `holds_custody(route, pool, lane_recorded)`
(`slots/page_io.rs`) selects the endpoints the stop checkpoints. An endpoint that was never registered
holds nothing and is skipped with the probe `inference.kv_page.stop_checkpoint_skipped`, so a stop
with nothing at risk reports "all state durable", not a false alarm. `SlotDirectory::existing` is the
read that never inserts, so a stop never creates the custody it then fails to save. I1 (§4) is the
invariant. A stop's phases are bounded (2 s each, `runtime.rs` `shutdown_within`).

**Proposed:** her memory is written first at a stop, with no git at stop time, through a volatile
writer stage. Today a resident's git commit inside the cognition save can outlast the phase bound.

## 9. Measurement

Each concern is measured once, where it happens, and published as a typed snapshot or probe:
- **`EngineSnapshot`** (`Record`): busy permille and the longest idle gap, over a window closed by
  the first step after it ends. A step's span is the engine thread's time on it, including the wait
  for the device.
- **Turn rows** (`Record`): per turn, tokens, prefill reused and fresh, waits and the end.
- **`inference.turn.wrote_nothing`**: a turn that wrote no text, with its token count, end, matched
  stop, reasoning and written lengths, and a 160-character tail of the raw output.
- **`inference.decode_permit.turn_waited`**: on the external-driver path, how long a foreground claim
  waited for a permit.
- **Simulation first**: every design section has its simulator, and every fault becomes a replay
  scenario before it is fixed live (SUBSTRATE §3).

## Former section numbers

Code comments cite this chapter by its old numbers. They resolve as follows:

| Old | Now |
|---|---|
| 4.0-4.6 (the serving engine) | §2, §5 |
| 4.7 (memory conserved across minds; 4.7.3 placed checkpoints; 4.7.13 a condemned engine is suspended, never killed) | §5, §8 |
| 4.8 (one admission owner) | §4 |
| 4.9 (I1-I5, P1-P5, G1-G7) | §4 |
| 4.10 (P3-P6, P4a) | §4 |
| 4.13, 4.15 (the frame loop) | §3 |
| 4.14 (personas stream like Nanite) | §5.1 |
| 4.16 (a mind is one object; the protocol) | §2, §3 |
| 4.17 (the engine as objects) | §2 |
| 5 (a mind's context; faults 1 and 2) | §6, §7 |
| 2, 3, 7 (optimization, substrate, measurement) | SUBSTRATE.md |
| 8 (the plan) | `CLAUDE.md` order of work, and the cards |
