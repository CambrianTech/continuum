# In-process learning on the persona engine

Status: design, 2026-11-18. Owner: Fable (engine, llama crate); Codex (serving, admission callers).
Read with [ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)
and CONTINUUM-ENGINE.md 4.14 (the persona engine: one mind decodes at a time).

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

## The driver's one queue

`Job::{Turn, Learn}` on one mpsc queue, one thread, one context (agreed 2026-11-18):

- A `Turn` runs as today: switch, prefill her new tail, decode.
- A `Learn` binds her and her shadow, calls `learn_begin`, then runs one chunk per `step`. Between
  chunks the driver serves any waiting `Turn`, so a turn waits at most one chunk (`max_chunk_us`),
  never a whole reply. `max_chunk_us` is a target, not a bound: her first-token wait also includes the drain, the rebind, any queued turns and decode, and that added delay is proven by contention and fault traces. When a waiting turn needs the context exclusively, the driver calls `drain`,
  which stops new gradient chunks and restores the served tail. Steps already committed stay
  committed.
- Nothing decodes in parallel. A learn chunk and a decode never overlap; they alternate.

## How the trainer reaches it

The trainer calls a `LearnPort` trait with one method per verb it uses today (`seat`, `learn`,
`release`). The in-process `Scheduler` implements it, and the trainer moves to it in ONE production
cutover. No engine means a typed `Unavailable`, never a runtime fallback to HTTP. The HTTP
`LearningEngine` and `live_lane` are deleted in that cutover or right after it.

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
