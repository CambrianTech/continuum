# Locks and FIFOs: the audit (2026-10-31)

Joel, 2026-10-31: "Locks and fifo are your disability. Find this." and "If inference isn't nearly
90% actively on something is bad or wrong."

The target is not GPU busy time. The M5 GPU read 94% busy (ioreg, 60 s, build 526bbe418), but
30 of 95 prefills in an hour re-prefilled more than 25k tokens and took 58% of prefill time.
The target is **the share of engine time spent on new tokens**, with every mind's work able
to reach the engine without waiting on another mind's lock, or on its own.

Read-only sweep of canary 11f853b8d (two passes: engine/serving/kernel and
mind/cognition/core/stores). 1,387 lock and queue primitive sites exist outside tests. Most are
harmless. Listed here are the ones that serialize minds. Each row has its receipt; the paths
are under `core/`.

## The rule these break

A guard or permit lives only as long as the resource it guards is in use. A mind waits on an
event, never by holding something another mind needs. Work is submitted to queues with
priorities, and one dispatcher hands out the resource ([[queues-and-signals-never-permits-held-across-work]],
CONCURRENCY-STYLE-GUIDE.md, plan goal 9).

## Ranked findings

### Hot path: on every generation

| # | Where | What is held / serialized | Replace with |
|---|---|---|---|
| 1 | `continuum-engine/src/inference/resource_admission/lanes.rs:410-497`, taken at `continuum-cognition/.../llm_deliberation_faculty/request.rs:216` | The shared lane permit is held while waiting on the mind gate (`turn_admission.rs:289`), the all-pinned slot wait (`:346-381`, p50 31 s live), engine readiness up to 240 s (`:305-309`), the slot permit (`:394`) and KV save/restore (`:437-481`). Every idle wait also takes a lane from another mind. | Take the slot (an event on pool release), then the lane, for the generation only |
| 2 | `continuum-engine/src/inference/slots/citizen_slots.rs:8-10`, `kv_slot_pool.rs:8-11` vs `lanes.rs:354-359` | n lanes but n-1 citizen slots at n>=3: a directed turn holding the reserved lane still waits on "every slot was pinned"; lending the reserve does the same | One scheduler over slots; directed is a priority, not a reserved idle resource |
| 3 | `continuum-engine/src/inference/slots/mind_gate.rs:28,42,51`; warm-ahead `turn_admission.rs:244-254` | Per-mind Semaphore(1) taken after the lane; warm-ahead holds it through save+restore, so her real turn waits behind her own speculation while holding a lane | One actor per mind; it asks for the lane only when its own work is ready |
| 4 | `continuum-engine/src/ai/openai_adapter/runtime.rs:60` | A second FIFO semaphore under the lane: a turn that waited on a slot rejoins at the back; the shrink reconciler parks in it one acquire at a time | Delete (the lane already bounds it) |
| 5 | `continuum-engine/src/inference/resource_admission/progress.rs:171-196` | One global ticket ledger, one head of line, for every mind's non-directed calls | Per-mind queues, weighted fair pick (priority = activity temperature, aging) |
| 6 | `continuum-engine/src/inference/slots/saved_ledger.rs:127-145` (from admission and `TurnAdmission::drop`) | parking_lot Mutex held across `fs::write` + directory fsync, synchronously on a tokio worker, 3-4 times per page switch | One ledger-writer task fed a coalescing watch |
| 7 | `lanes.rs:541-543`, `serving_lane_permit.rs:39-42` | Lost wakeups: `notified()` created after the try and never enabled; `notify_waiters` fires before the permits drop; both fall back to a 1 s sleep | `enable()` before trying; notify after release |
| 8 | `continuum-engine/src/ai/adapter/global_registry.rs:15`; `continuum-core/src/commands/ai/generate.rs:168`; `should_respond.rs:230`; `tool_embedding.rs:296,390` | Global tokio RwLock read guard held across a whole generate/embed; a queued writer (DMR tick, persona spawn) then stalls every mind's reads behind the longest decode | ArcSwap snapshot; select, drop, then await (as `generate_response.rs:318` does) |

### Hot path: the turn itself

| # | Where | What is held / serialized | Replace with |
|---|---|---|---|
| 9 | `continuum-mind/.../turn_cycle.rs:176-193,249,286`; `burst_fingerprint.rs:314,323-331,564`; pool `lanes.rs:374-395` | Ambient permit (pool lanes-1) held across a whole `drive_to_settle`: board reads, RAG compose, every tool batch, git. Another mind's un-addressed turn finds the pool empty and is skipped. The self-tick takes it before checking whether anything changed. | Delete; the per-generation lane bounds decode |
| 10 | `continuum-mind/.../serve_persona_loop_inner.rs:103,162,401`; `burst_fingerprint.rs:564`; `apply.rs:268` | One mind is one loop: a self-tick turn runs unlimited acts and tool batches of up to 30 min inline; a direct message waits behind the whole turn | Turns run as tasks with handles; a directed message is an event checked between acts ([[her-work-runs-as-tasks-with-handles-and-wakes-like-claude-code]]) |
| 11 | `card_hands.rs:439`, `act_question.rs:431`, `workspace_transfer.rs:66` | Git commit+push (up to 60 s) awaited before her reply is published | Spawn the sync after the say; a move waits on its handle |
| 12 | `persona_airc_runtime_2.rs:496`, called from `turn_cycle.rs:163`, `card_hands.rs:51`, `act_question.rs:144` | Every subscribed room's board read in series, 2-3 times per turn | A per-mind claims projection updated by `BoardChange` events |
| 13 | `act_observe/apply.rs:168,188,688`; `disk_change.rs:83` | Git fingerprint, effect mark, batch, git fingerprint, all in series | Overlap the independent steps |
| 14 | `turn_cycle.rs:62`, `act_question.rs:399` -> `continuum-genome/.../composer.rs:225,269`; `gene_trial.rs:348`; `signature.rs:253` | Two JSON files re-read with sync `std::fs` on the async thread, plus an embed, every turn | Cache on change; run beside compose and recall |
| 15 | `serve_persona_loop_inner.rs:311-352`; `airc_source.rs:389,432,488`; `unified.rs:236` | admit, obligation write, compose (airc page, room tail, cursor), genome, claims x2, close-ended claims: mostly independent, all in series | `try_join!` the independent reads |
| 16 | `turn_context.rs:138-144` | Per-mind tokio Mutex held across `compose_for_turn` (IPC, queries) | Take the sources out, release, then await |

### Cross-mind, off the turn but blocking it

| # | Where | What is held / serialized | Replace with |
|---|---|---|---|
| 17 | `continuum-kernel/src/orm/sqlite/open_connection.rs:84-87,125-131`; `continuum-store/.../data_state.rs:96`, `adapters.rs:42` | One writer Mutex per DB (main.db shared by all minds); readers round-robin regardless of busy, so one slow query blocks the next caller; a global init lock queues per-persona opens | A free-reader pool; one writer task per DB with batching |
| 18 | `continuum-mind/src/work/card_staging.rs:267,272` | Global Mutex held across `gh repo clone` (up to 900 s): claims of different repos queue | A per-repo keyed gate |
| 19 | `continuum-mind/.../card_carry.rs:28-56`; `continuum-core/.../card_carry.rs:44` | Sweep over all minds x held cards, each git push in series, under `CHECKOUT_SYNC` that her own end-of-turn sync needs | One carry task per checkout |
| 20 | `continuum-engine/src/inference/slots/kv_slot_pool.rs:455`; `engine_generation.rs:149,175,393-436` | Endpoint RwLock: each generation holds a read guard for its whole call; a relaunch queues the writer without closing readiness, so new admits park holding lane, permit and gate until the longest act ends; then residents are saved one by one | Close readiness first, drain on an epoch watch, save concurrently |
| 21 | `continuum-kernel/src/system_resources/disk_reporters/registry.rs:444-470` | Mutex held across SQLite write and fsync: every citizen's cargo-target admission node-wide | A per-path gate; persist off the lock |
| 22 | `continuum-core/src/ipc/command_ingress.rs:633` | Non-generation remote commands answered inline by one pump, first come first served | Spawn with bounded admission |
| 23 | `continuum-kernel/src/runtime/message_bus.rs:349-389` | Publisher awaits each subscriber's `handle_event` in turn | Deferred delivery on the broadcast tier |
| 24 | `a_i_provider_module.rs:16`, `airc.rs:196`, enforced `shutdown_phase.rs:484` | Module-level FIFO limits (legacy `ai/*` = 1, chat publish = 4) | Limit per resource, not per module |

### Background ticks and polls standing in for events

| # | Where | What | Replace with |
|---|---|---|---|
| 25 | `continuum-mind/src/modules/cognition.rs:187,218` -> `own_speech_persisted.rs:91,94` | Every 15 s, every resident's whole working memory saved with `sync_all` + parent fsync, changed or not (240/h per resident live) | Dirty flag; save on change |
| 26 | `continuum-kernel/src/resources/daemon.rs:71`, `serving_daemon/consumer.rs:120-127`, `plan_serving.rs:526,553,575` | A 1 s tick recomputes the serving plan 3 times inline (~3 probes/s live; answers flap 1-3 lanes, 24k-71k windows) | Recompute on lease, working-set or catalog change, with two-way hysteresis |
| 27 | `airc_persona_conversation/polled.rs:16,684,210` | A 60 s catch-up per mind pages every room in series inside the pump's `select!`, delaying live frames | One durable gap feed per core |
| 28 | `beat_outcome.rs:57,65`; `burst_fingerprint.rs:323-347`; `wake_select.rs:63-90` | A 15-240 s heartbeat rebuilds context to *detect* change by fingerprint | Wake on feed and board events |

## Order of repair

1. **The generation's admission becomes one scheduler** (rows 1-5, 7, 9; Codex, card 07484926):
   - a generation is a job submitted with her priority;
   - one dispatcher hands out a slot, then a lane, only when both can run;
   - no reserved idle lane, no second FIFO, no permit across a tool or a wait;
   - receipts: `slot_affinity.waited` p50 near 0, and lane grants by priority.
2. **Her turn runs as tasks** (rows 10-16; Fable):
   - independent reads joined;
   - git sync spawned after the say;
   - a directed message is an event between acts;
   - the ambient permit deleted.
3. **Shared stores and registries off the lock** (rows 8, 17-24):
   - ArcSwap for the registries;
   - a reader pool and a writer task per DB;
   - per-key gates.
4. **Ticks become events** (rows 25-28):
   - save on change;
   - plan on change;
   - one gap feed.

Each slice lands with its probe read on the live node: lane wait, slot wait, the share of
prefill tokens that are new, and acts/hour per resident. A source-hygiene ratchet follows the
first slice: no guard or permit alive across an `.await` on a turn's path
(`clippy::await_holding_lock` covers std/parking_lot; the ratchet covers tokio guards and
semaphore permits).

## Not harmful, checked

- Perception faculties join (`workspace_cycle_2.rs:84`).
- Tool batches run concurrently (`command_executor.rs:610`).
- The one-feed fan-out is `try_send` under a short lock.
- Served generations are spawned.
- `PerKeyGate` is per key.
- The measured-hold defer is event-driven.
- inference-grpc is disabled.
- `ConcurrentProcessor` and `drain_all_personas` have no live caller.
