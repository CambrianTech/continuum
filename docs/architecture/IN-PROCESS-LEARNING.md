# In-process learning on the persona engine

Status: design, 2026-11-18. Owner: Fable (engine, llama crate); Codex (serving, admission callers).
Read with [ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)
and CONTINUUM-ENGINE.md 4.14 (the persona engine: one mind decodes at a time).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [Why this comes before deleting the lanes](#why-this-comes-before-deleting-the-lanes)
- [What moves, and where](#what-moves-and-where)
- [A learn is frames of work, like a turn](#a-learn-is-frames-of-work-like-a-turn-l2-proposed-2026-11-20)
- [Slices, each with its validator](#slices-each-with-its-validator)

## Why this comes before deleting the lanes

Today a mind learns only inside llama-server. The trainer
(`continuum-dreaming/src/modules/training_trigger.rs:221`) finds its engine through
`lane_registry::live_lane()`, which needs a llama-server process. The Rust side POSTs `/learn`
(`continuum-engine/src/inference/llama_server/learn_step.rs:60`), and the fork's C++ runs the step
on the server's own context (`tools/server/server-context-learn.inc`, 647 lines at pin `a88f255cc`:
`llama_opt_init`, then `llama_opt_learn_begin` and `llama_opt_learn_graph_step` / `_drain`). The
llama crate has no wrapper for any of it.

Deleting the lanes and llama-server first would stop every mind from learning. Learning is the goal
(a reviewed correction kept across restart, applied to unseen tasks), so it moves in-process first.

## What moves, and where

The fork's API already has the shape the persona engine needs. `llama_opt_learn_begin` trains one
reply on one sequence of a serving context. Each `llama_opt_learn_step` / `_graph_step` runs at
most one chunk. Other sequences may decode between steps. Afterwards the reply is restored under
her stable gene set, so serving's state is what it was.

| Today (C++, in llama-server) | Moves to | Owner |
|---|---|---|
| The `llama_opt_*` calls | `llama` crate `safe/learn.rs`: a `Learner` over `&mut Context`, typed `LearnRefusal` from `llama_opt_failure`, the status as an enum | Fable |
| The residency mirror and its admission callbacks (`static_admission`, `workspace_admission`) | A Rust `LearnAdmission` trait the callbacks call. Codex implements it over `training_admission` and the ResourceDaemon: measured incremental bytes, held until the step completes | Fable (trait), Codex (implementation) |
| The learn ledger (grants: `period_ms`, `max_chunk_us`, `share_ppm`; committed steps) | The time-share policy stays with the existing grant issuer. The driver consumes ONE versioned grant and measures its elapsed chunks and drains. There is no second quota and no dedicated lane | Codex (issuer), Fable (driver) |
| Shadow selection per mind (`llama_opt_init` with her shadow adapter) and the preflight (`llama_opt_measure_shadow_graph`) | The driver, when it binds her for a learn | Fable |
| `POST /learn`, `/learn/grants` and the residency endpoints | Removed with llama-server, after the trainer reaches the scheduler | Codex |

## A learn is frames of work, like a turn (L2, proposed 2026-11-20)

The engine is concurrent everywhere except inference, which runs like a CPU core. A **Core**
(`PersonaEngine` with `pipeline::Roles`: active, next, previous) runs one mind at a time. Her
native state is the register file: the next mind is staged in and the previous one retired out
while the active one runs, so the minds seem parallel. How many Cores a node has comes from the
engine's list of Cores, under measured admission (one on the M5; a larger node may run more),
never from parallel decode inside one Core. Learning is not a special mode that holds the engine.
It is work of the mind active on a Core, scheduled and switched like her decode. The Core's
driver is its own thread (`persona_scheduler/driver.rs`), and nothing else waits on it: callers
submit and subscribe, and the thread advances one item by one quantum per frame.

**Flow, one resident learning while another becomes ready to speak:**

1. **Submit.** The dream stage calls `LearnPort::learn(LearnRequest)`. That pushes
   `Submission::Learn` onto the driver's existing queue and returns at once with a `Subscribes`
   to the outcome (STAGE-CONTRACT artifacts). The caller never awaits the engine. The completion
   re-triggers its stage. `LearnRequest` carries what the frame needs, already granted: her
   persona, the reply (tokens, labels, restore tail), her shadow adapter, its `OptimizerLease`, and
   the `ReplyAdmission` from Codex's `LearnAdmission` over the ResourceDaemon. The driver never
   asks the authority mid-frame.
2. **One worklist.** The driver holds `Vec<Work>`, with `enum Work { Turn(Turn), Learn(Learn) }`.
   `frame::pick` ranks both with the same `Candidate` (bound, remaining work, waited). A frame is
   one quantum: a prefill chunk, one token, or one learn chunk of the active mind (`llama_opt_learn_step`, or
   `graph_step` when a turn has waited past its target).
3. **No borrow across frames.** Today `LearnRun<'l>` holds `&mut Learner`, and so the context, for
   a whole reply, so no turn could run before the reply ends. L2 makes the run an owned value,
   `ReplyInProgress`: the native result handle, sequence, shadow key, steps before, and state and
   buffers. It lives in `Work::Learn`. `Learner::new(&mut ctx).step(&mut reply)` borrows the
   context for exactly one chunk. The borrow ends when the call returns, before the next frame
   picks.
4. **Switching a learning mind.** P learns while she is active on the Core. When Q becomes ready,
   the Core does what it does for any switch: Q is staged as next while P's chunks run, and the
   handoff happens at a frame boundary. Today the fork keeps one learn reply per context, outside
   P's saved state, so P cannot be retired mid-reply with her learn intact. Two steps follow.
   - **L2:** before P is retired, her reply drains (`drain` stops new gradient chunks, and
     committed steps stay) and its restore chunks run as P's last frames before the handoff. Q's
     staging overlaps them, so the only added delay is the remaining restore chunks, which are
     measured.
   - **L2b, the full context switch:** the reply's native learn state becomes part of her saved
     state (a fork change), so P is retired and later restaged mid-reply like any process, and
     nothing drains.
   
   Validators: Q's first token waits at most one chunk plus her staging, and both minds' native
   prefixes are intact afterwards. For L2b, a reply retired and restaged mid-way commits the same
   steps as one run straight through.
5. **Complete.** `finish` maps `Finished` to the artifact through a completion outbox (the
   `Emit`/`Record` pattern):
   - `Learned` and `Unchanged` hand the admission back.
   - `Uncertain` is never published as learned.
   - `Failed` quarantines the context, and `recover` then runs as later learn frames after her
     retained shadows are released.
6. **Engine availability is an event.** The scheduler's `watch<EngineState>` is the only source.
   The dream stage subscribes: `Ready` re-triggers it, and `Stopped`/`Refused` make
   `LearnPort::learn` answer a typed `Unavailable` at once. There is no polling and no
   `live_lane()`.

**Waits removed:**
- The trainer waiting on an engine: it subscribes instead.
- A turn waiting out a whole reply: chunks interleave.
- The driver waiting on admission: it is granted before submission.
- `recover_pass` returning early with no lane: the engine-state event replaces it.

**Deleted in L3:** the whole-reply `LearnRun` borrow, `POST /learn` and its ledger and residency
endpoints, the HTTP `LearningEngine`, `lane_registry::live_lane`, and `training_trigger`'s lane
lookup. Gene catalog and paging over HTTP (`llama_server/gene_pages.rs`) go with llama-server in L4.
The trainer's surface becomes `LearnPort`: `learn`, `seat`/`release` her shadow, `save` a checkpoint.

## Slices, each with its validator

1. **L1, llama crate wrapper.** `Learner` and `LearnRefusal` with no policy. Validator: on a 0.5B
   model, one reply trains, its loss drops, and the sequence holds exactly what it held before.
2. **L2, `Job::Learn` on the driver.** Chunks interleave with turns. Validator: a learn and two
   minds' turns on one context. Each turn's first token waits at most one chunk, and each mind's
   native prefix is intact afterwards.
3. **L3, the trainer through `LearnPort`.** Validators, with no llama-server running:
   - the in-place trainer's existing tests pass;
   - her native prefix is equivalent before and after a learn;
   - a committed step survives an interruption and its retry without being applied twice;
   - held-out quality holds after a reload, alongside throughput on the 35B model.
4. **Then:** delete llama-server's learn endpoints, the HTTP adapter, `live_lane`, and the lanes
   (Codex's serving half, Fable's engine half).

Live gate after L3: one admitted learning step on a live resident, with throughput held, on the
M5 and on BigMama.
