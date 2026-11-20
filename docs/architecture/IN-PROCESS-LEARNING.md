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
| `POST /learn`, `/learn/grants` and the residency endpoints | Removed with llama-server, after her dream stage dispatches to the Core | Codex |

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

**Learning is hers.** A citizen's genome is part of her, and learning is her own lifecycle:
live, work, remember, dream, learn, test (README, "They truly learn"). The engine already models
this: `PersonaUnit` is her five parts, moved only as a whole: blocks, tokens, **genome**,
**learning**, decode. Her `genome` part holds her genes, the shadow gene she is growing, its
optimizer state and its lineage. Her `learning` part holds a lesson in progress. Nothing outside
her pushes examples into an engine or grades her ordinary work. Her dream faculty (a stage of her
mind, STAGE-CONTRACT's dream consumer) chooses what experience to consolidate, and the Core runs
that lesson as her work while she is active on it, as it runs her turns.

**Flow, one resident learning while another becomes ready to speak:**

1. **She dreams.** P's dream stage turns settled experience into a lesson (tokens, labels, restore
   tail) and dispatches it as her work: `Submission::Learn` on the Core's existing queue. It
   returns at once with a `Subscribes` to the outcome (STAGE-CONTRACT artifacts). The outcome
   re-triggers her dream stage, and nothing waits. The memory her lesson needs, the shadow's
   optimizer bytes and the reply workspace, is admitted before dispatch by the one resource
   authority (Codex's `LearnAdmission` over the ResourceDaemon). The authority parks the demand,
   reclaims lower-value bytes through their owners (cache spills to disk), and grants on
   confirmed release, so the Core never asks mid-frame. On BigMama, 11.5 MiB was refused 29
   times with 302 MiB physically free (Nov 19, 14:14-14:29Z); a governed system finds it.
2. **One worklist.** The driver holds `Vec<Work>`, with `enum Work { Turn(Turn), Learn(Learn) }`.
   `frame::pick` ranks both with the same `Candidate` (bound, remaining work, waited). A frame is
   one quantum: a prefill chunk, one token, or one learn chunk of the active mind (`llama_opt_learn_step`, or
   `graph_step` when a turn has waited past its target).
3. **No borrow across frames.** Today `LearnRun<'l>` holds `&mut Learner`, and so the context, for
   a whole reply, so no turn could run before the reply ends. L2 makes the run an owned value,
   `ReplyInProgress`: the native result handle, sequence, shadow key, steps before, and state and
   buffers. It lives in her `learning` part, so it moves with her.
   `Learner::new(&mut ctx).step(&mut reply)` borrows the context for exactly one chunk. The borrow ends when the call returns, before the next frame
   picks.
4. **Switching a learning mind.** P learns while she is active on the Core. When Q becomes ready,
   the Core does what it does for any switch: Q is staged as next while P's chunks run, and the
   handoff happens at a frame boundary. Today the fork keeps one learn reply per context, outside
   P's saved state, so P cannot be retired mid-reply with her learn intact. Two steps follow.
   - **L2:** before P is retired, her reply drains (`drain` stops new gradient chunks, and
     committed steps stay) and ALL of its remaining restore chunks run as P's last frames before
     the handoff. Q's staging overlaps them, but Q still waits for every restore frame, then her
     own bind, prefill and first decode. The restore tail is the longest piece that cannot yield,
     so it is the floor on Q's wait, and L2 only stands until L2b removes it.
   - **L2b, the full context switch:** the reply's native learn state becomes part of her saved
     state (a fork change), so P is retired and later restaged mid-reply like any process, and
     nothing drains.

   L2 validators:
   - **Delay.** Measure Q's ready-to-bind and ready-to-first-token against the same Q turn with
     no learn on the Core. Trace P's last chunk, restore, Q's staging, bind, prefill and first emit
     separately, including a case with a long restore tail.
   - **Weights.** P and Q use distinct adapters. After the learn and restore, Q's context gene
     binding is correct and her logits match an uninterrupted decode bit for bit.
     `UnifiedTier::applied_genes` caches the binding by `Arc` pointer, so matching prefix lengths
     alone would miss wrong weights.
   - **Prefixes.** Both minds' native prefixes are intact afterwards.

   L2b validator: a reply retired and restaged mid-way ends with the same shadow weights and the
   same optimizer step count as one run straight through.
5. **Into her genome.** `finish` writes the outcome into her own parts and publishes it to her
   dream stage through a completion outbox (the `Emit`/`Record` pattern):
   - **`Learned`:** her shadow gene's committed steps and loss, appended to its lineage in her
     `genome` part. The candidate still has to earn adoption: her phenotype is evaluated before
     the gene joins her genome (README: "It has to earn adoption").
   - **`Unchanged`:** nothing is committed.
   - **`Uncertain`:** never kept as learned.
   - **`Failed`:** quarantines the context. `recover` then runs as her later frames, after her
     retained shadows are released.
   
   Every outcome hands back its admission.
6. **Serving her is an event.** The Core's `watch<EngineState>` is the only availability source.
   Her dream stage subscribes: `Ready` re-triggers it, and `Stopped`/`Refused` answer her dispatch
   with a typed `Unavailable` at once. There is no polling and no `live_lane()`.

**Waits removed:**
- Her dream waiting on an engine: it subscribes instead.
- Another resident waiting out a whole reply: chunks interleave.
- The Core waiting on admission: memory is granted before dispatch.
- Learning silently not running with no lane: the Core's state event replaces the lookup.

**Deleted in L3:**
- The whole-reply `LearnRun` borrow.
- `POST /learn` and its ledger and residency endpoints.
- The HTTP `LearningEngine` and `lane_registry::live_lane`.
- The outside trigger that finds a lane and pushes examples into it (`training_trigger`'s
  `recover_pass`/`learn_in_place`). Its curriculum selection moves into her dream stage.

Gene catalog and paging over HTTP (`llama_server/gene_pages.rs`) go with llama-server in L4; her
genes are then catalogued and paged through her `genome` part on the Core.

## Slices, each with its validator

1. **L1, llama crate wrapper.** `Learner` and `LearnRefusal` with no policy. Validator: on a 0.5B
   model, one reply trains, its loss drops, and the sequence holds exactly what it held before.
2. **L2, learning on the Core's driver.** A learn's chunks interleave with turns; a switch drains
   the learn first. Validators: the delay, weights and prefixes checks in
   [A learn is frames of work](#a-learn-is-frames-of-work-like-a-turn-l2-proposed-2026-11-20).
   **L2b** (fork) saves the learn state with her registers so nothing drains; validator: the same
   shadow weights and step count as one uninterrupted run.
3. **L3, her dream stage dispatches her lessons to the Core.** The outside trigger and the HTTP
   learning path are deleted. Validators, with no llama-server running:
   - the in-place learning tests pass against her dispatch;
   - her native prefix is equivalent before and after a learn;
   - a committed step survives an interruption and its retry without being applied twice;
   - held-out quality holds after a reload, alongside throughput on the 35B model.
4. **Then:** delete llama-server's learn endpoints, the HTTP adapter, `live_lane`, and the lanes
   (Codex's serving half, Fable's engine half).

Live gate after L3: on the M5 and on BigMama, a resident's own dream commits a learning step into
her shadow gene with throughput held, and that gene then earns or fails adoption on her phenotype.
