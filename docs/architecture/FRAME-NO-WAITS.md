# The frame: no waits, constant swapping

Status: design draft, 2026-11-22, Cormac (IntelMac), from a session with Joel. Builds on SAGE
(`docs/architecture/SAGE.md`, `core/continuum-sage`, on `fable/refusal-at-door`, not yet on canary)
and CONTINUUM-ENGINE section 4. Owners: Fable (engine, SAGE), Codex (scheduler), Cormac (learner).

Joel, 2026-11-22: "Slot switching is instantaneous." "We are a game engine!" "Backwards. Your waits
are dumb. You don't wait period." "Swapping is regular just constantly swap. You have to for something
like live video calls or really any team thing." "Zero serial nature in this whole thing." "The core
and events of sage need to be regular." "Swap like as soon as the step is done even mid inference, like
every 2 seconds would be fine. Maybe even sub second if it's truly 20ms."

## 1. The law

Nothing waits. A worker never awaits another worker, a lock, a grant, a slot or memory. Under SAGE's rule
the only serial work is a queue insert or a state read; everything else is a component that hears an
event, does its own work, and says what it has. When something is not ready, the consumer was woken too
early: it should have subscribed to the event that says it is ready.

Every wait in the tree today is a defect to convert, never to shorten. A shorter wait (a preemptible grant,
a faster lock, a timeout) is still the wrong model.

## 2. Two things swap, constantly, on their own cadences

| Layer | What it is | What swaps | Measured swap (the M5) |
|---|---|---|---|
| Mind | one activity's live state, event-driven, holds nothing between events | its KV pages (seq save/restore, `seq_cp` for shared prefixes) | switch p50 26 ms, max 263 ms (SAGE, aa734cc01c) |
| Persona | identity: genome adapters, learning shadow, buckets | adapters attached to the engine, her shadow seated | persona swap p50 about 6 ms (validator `persona_switch`) |

A persona can stay resident while her minds rotate; minds rotate across personas without reloading
adapters. Swapping is the normal mode, not an exception path: a live video call or any team activity
keeps every participant's minds and personas live at real-time cadence. Nothing decides whether a swap
is "worth it"; the only question is that each swap is fast and its spread is small.

## 3. The frame

What a mind passes through today before it generates (canary c0fa92246, read 2026-11-22), every one a
serial wait: (1) `turn_ingress::admit` (serve_persona_loop_inner.rs:134); (2) the window grant,
`deliberation.window_grant` (admission_clock.rs:191; grants republished on a 5 s tick and on events,
serving_daemon.rs:64, live_pool.rs:217); (3) the lane semaphore, `deliberation.lane` (lanes.rs:46-90);
(4) one act per mind, a FIFO semaphore of 1 (mind_gate.rs:63-84); (5) the decode permit, ONE generation
per endpoint (endpoint_concurrency.rs:42-50, #6114: four minds batched read 2.1-2.4 tok/s each against
about 80 for one); (6) endpoint readiness (turn_admission.rs:431); (7) the slot pin, `released.notified()`
when every slot is pinned (turn_admission.rs:476-526); (8) page save of the evicted mind then restore of
hers over llama-server HTTP (turn_admission.rs:604-640; live median save 3.05 s, restore 1.38 s, on the
server's main loop, stalling every stream, CONTINUUM-ENGINE 4.1b); (9) first bytes (lane_send.rs:309).
The fast swap measured on SAGE (persona swap p50 about 6 ms) is the persona engine's
`llama_persona_save/bind` (continuum-persona-llama), which the live path does not use yet.

The frame replaces all nine:

The Core is the one engine. It runs frames.

- **Every frame advances every mind with pending work.** Minds with a pending turn are ranked by need,
  then waiting age (nobody starves), then switch cost (SAGE's priority tree). As many as the engine's
  measured sequence capacity (#5188) decode together in one batch. Prefill chunks interleave with decode.
- **Round robin at the next chance.** Swap is well under a second (the Core's switch p50 26 ms), so
  after every step the engine moves to the next ready mind in round-robin order, mid-inference
  included. Simultaneous personas share the engine step by step, like an OS on one core. The one
  threshold: a mind whose remaining work is shorter than a swap finishes first, because switching would
  cost more than it saves. That threshold is the node's measured swap time (p90, per KV size, updated
  at every swap), never a constant; a swap that measures slow shows on the receipt and raises the
  threshold by itself. Urgency (steering, a human addressing her) only orders the round, it never
  holds the engine.
- **Memory streams ahead of need.** A mind's event reaches the Prepare part, which pages its KV toward the
  fastest tier before its frame (SAGE's cache hierarchy: VRAM, RAM, flash, cold disk, detected per
  machine). A fetch at the moment of use is a cache miss, and every miss is loud.
- **No exclusive window.** There is no grant to hold, hand off or wait for. Today a `GrantLease` holds a
  window for one whole `TextGenerationRequest` (Codex, 2026-11-22), and one request can be minutes; that
  unit goes. The scheduler's unit is the step.

## 4. Learning is frame work

A learning step is not a separate pass that seats a shadow, holds an engine lock and waits for memory.
It is one more kind of work in the frame:

- **Tiles, not whole examples.** The engine already serves a 63,488-token window in 32 GB by streaming
  ubatches through the KV cache. Learning streams the same way: forward fills the cache tile by tile,
  backward walks the tiles, recomputing each against the cached KV; a tile's compute is bounded by the
  tile, sized from the live free budget each frame. A tile always fits, so a learning step always has an
  answer. Today one example with reply_rows=33,157 asks for 4,226,258,182,784 bytes of CUDA compute
  (the 5090, 2026-11-22): the whole graph materialized at once.
- **The learner is a component.** Stimuli: accepted submissions (journal frames), the persona's presence,
  the Core's frame and free budget. Outputs: learning tiles submitted to the Core, learned positions,
  generations. Need: Content while she acts, Want while she rests with unlearned examples, Urgent for a
  reviewed correction. Its shadow pages in for its tile and out after, like a mind's KV.
- **One example never blocks a bucket.** Tiles make every example learnable; the journal keeps each
  row's own place. Today the bucket's oldest unlearned row (cb85bab6, seq 62) blocks 26,415 behind it,
  Kimi's reviewed correction included.

## 5. The census: where the waits are

Counted on canary 9a92a64f0, non-test source under `core/*/src` (`git grep`, 2026-11-22):

| Shape | Sites |
|---|---|
| `tokio::time::timeout` | 165 |
| `tokio::time::sleep` | 127 |
| `.lock().await` (async mutex held across work) | 73 |
| oneshot/channel `.await` on a reply | 67 |
| `tokio::time::interval` | 49 |
| `PerKeyGate`/semaphore `acquire(..).await` | 20 |
| `notified().await` | 19 |
| `spawn_blocking` (each a hidden serial hop unless it is a component's worker) | 202 |

The ratchet `waits_that_decide_flow` already counts 310 in six crates (core 118, engine 67, kernel 45,
mind 41, academy 27, live 12). Each shape has one conversion:

| Wait | Becomes |
|---|---|
| sleep / interval / timeout deciding flow | the event the code was polling for; a deadline a component chose is an `Alarm` on SAGE's one clock |
| a lock held across work | the state has one owner component; others read its published state (readers never wait) |
| a gate serializing writers of a key | one owner component per key; writers emit, the owner applies in its intake order |
| awaiting a reply | emit the request; the reply is an event the requester handles when it comes |
| awaiting memory, a slot, a grant | the frame: work is submitted with its size; the Core admits what fits this frame, tiles the rest |

## 6. The trainer, as the first conversion (Cormac)

Every serial point in `continuum-dreaming/src/modules/training_trigger` on canary c0fa92246, and what
it becomes:

| Today (file:line) | Waits on | Becomes |
|---|---|---|
| `in_place/pass.rs:175` `her_acts_done(persona).await` | her act ending | her presence state: the learner's need drops to Content while she acts; the Core ranks |
| `in_place/pass.rs:97,228` `submit_gates.acquire(&key).await` | the bucket gate | the bucket's owner component applies submissions, retirements, learned positions in intake order |
| `in_place/pass.rs:99,117,234`, `in_place.rs:233,616` engine `acquire(..).await` | the engine lock | the Core owns the engine; a learning tile is frame work |
| `in_place/pass.rs:208` `learn_slot(..).await` | a slot | the frame admits the tile |
| `MemoryShort` / `AwaitMemory` (pass.rs:126,141,201,271) | memory that may never exist | tiles sized from the frame's free budget |
| `in_place.rs:740-750` fence `interval(1s)` loop | a stop confirmation | the engine's stop is an event; custody settles on it or on the verified death event |
| `durable/training_trigger_state.rs:51` `receive.await` | an owned operation's result | the result is an event |
| `durable/training_trigger_state.rs:80` shutdown join | every operation | components rest on the shutdown frame; durable state is the journal |
| `serving_wake.rs` follower task, abort + await | a hand-spawned task | gone: the learner is a component woken by serving and journal frames |
| `durable/rekey.rs:282-283` two gates | both buckets | the bucket owners apply the move as one event each, ordered by the journal |

This also closes the 5090's shutdown overrun (drain.wake_stop never ending, six operations in flight
at a timed-out join): there is no follower to abort and no operation set to join.

## 7. What the engine must give (Fable)

Today in the fork (pin 126b9c1c3, read 2026-11-22):
- `update_slots` batches several slots only when they share task type and LoRA set (`can_batch_with`,
  server-context.cpp:480-486); one LoRA set is installed per batch (3368); slots with different
  adapters never share a decode; a holder slot is picked round-robin (3517-3528); there is no
  preemption.
- KV page switch exists (`server-context-page.inc`: `page_save` copies state on the loop, file IO on a
  worker pool, completion returns as a task). CONTINUUM-ENGINE 4.1b measured save 0.22 s, restore
  0.1 s, and page IO on the loop stalling every stream; SAGE measured the Core's switch at p50 26 ms.
- Live `/learn` builds ONE whole-reply graph on purpose (`graph_nodes=1`, src/llama-context-learn.cpp:
  277-281; llama.h:1982-1986), with recompute off (server-context-learn.inc:53-72), no CUDA
  FLASH_ATTN_BACK kernel (src/llama-context-opt.cpp:46-80), and a preflight that measures every reply
  row against the full 63,488 window (src/llama-context-opt-measure.cpp:19, 40-70). That is the
  4.2 TB.
- The offline walk already does the frame shape: `llama_opt_epoch` (src/llama-context-opt.cpp:416-925)
  chunks at n_batch, "chunk x window, never window x window", decodes under the shadow, trains chunks
  in reverse against cached KV with host-accumulated K/V gradients, and yields per chunk. Live learning
  should be that walk, not a new invention.

What the frame needs:

- one batch per frame across every admitted sequence (continuous batching), with prefill chunks
  interleaved;
- preempt and resume a sequence at a decode boundary by seq save/restore, with swap latency published
  per KV size (the quantum is chosen from it);
- a learning tile as frame work: the offline walk's chunked reverse pass run live, recompute on, the
  preflight measuring one tile against the real `n_kv`, so learning memory grows linearly in reply
  rows (a CUDA FLASH_ATTN_BACK kernel shrinks each tile further);
- the set decoded per frame is a measured policy, not a constant (CONTINUUM-ENGINE 4.10 P5): on the
  35B MoE at long context, batching many minds may not amortize KV reads (4.10 O1), while short
  contexts batch well (4.1b: 124 tok/s total at four streams); the policy picks from receipts;
- adapters attach per sequence per frame (persona swap) without reloading; mixing adapters inside one
  ubatch (SGMV) is the open gap (CONTINUUM-ENGINE 4.6, PATIENT-DOCTOR 9.6). Until it lands, a frame
  groups by adapter set and the switch between groups is the persona swap.

Did a fork change break swap? Checked 2026-11-22. SAGE's swap receipts (11-21) ran on
`fable/refusal-at-door`, whose pin b539d357 contains Cormac's fork #109 (ledger off the loop) and #113
(ticket fan-out). Neither touches page save/restore, slot selection or the decode batch. One cost on
the loop is Cormac's to remove: #109 rebuilds the ledger receipt JSON after every non-continuation
task, page-IO completions included; it should publish only when the ledger revision moved.

## 8. Contradictions this settles

- Batch every ready mind vs one mind decodes at a time (branch 4.13-4.15, IN-PROCESS-LEARNING): neither
  as a constant; the per-frame set is the measured policy above, and the quantum keeps the rest moving.
- Learning between decode batches vs in the same ubatch (PATIENT-DOCTOR 3 vs 9.2): learning tiles are
  frame work; whether a tile rides the serving ubatch or alternates with it is measured per node, never
  a wait either way.
- "Never preempt" (INFERENCE-SCHEDULING) vs preempt at a decode boundary (4.1c): every mind yields at the
  quantum; urgency only orders who runs next.

## 9. Order of work, each with a validator first

1. Census as a ratchet: one count over all the wait shapes in section 5, falling every week (extends
   `waits_that_decide_flow`).
2. SAGE lands on canary (Fable's line), so every conversion has its parent.
3. The quantum on the Core: replay the 5090's 823 s window_grant capture; gate: no mind waits more than
   one quantum plus one swap.
4. Learning tiles in the fork, validated on a captured 33,157-row example: compute bounded by the tile,
   same loss as a whole-example step on a short example.
5. The trainer as components (section 6); gate: Kimi's b38e0149 learned on the 5090 while residents act,
   act p50 unchanged; zero waits in `training_trigger` by the census.
6. Every other crate by the census, owner by owner.

Each step: a seconds-scale validator on captured sessions first, then a 15-minute live read before and
after (act p50/p90, swaps per minute, swap p50/p90, frame lag, learned examples per hour).
