# Benchmark-as-Room + the Agentic `observe` surface

**Status:** design locked 2026-07-17 (co-designed with Joel). Not yet built.
**Relates to:** #123 (positronic benchmarking), #141 (positron telemetry widgets),
#170 (streaming turn coalescing), the med-bay/proctor arc.

## The principle

An **activity is a room**. A benchmark run is an activity, so it is a room — a
joinable tab. The room is the **one truth**; every consumer renders it in its own
idiom. There is no operator side-channel (log-grep, `eval-status` polling): if a
human watches via a positron widget and an agent watches via a log, we've built
two truths. Everyone subscribes to the **same data**.

Design **agent-first = information-first**: an agent has no UI to hide behind, so it
forces the question "what actually matters here, as data?" Get the information model
right and the widget can't be wrong — it's a projection of a model already validated
for substance. Benchmarks/academy are the ideal proving ground: information-dense,
gradeable, crisp model.

## The information model (UI-free)

A benchmark activity is **three subscriptions**. The positron regions, the agentic
`observe` skill, and a proctor-persona's perception are all just renderers of these.

| Region (human) | Subscription | Agent idiom | Persona idiom |
|---|---|---|---|
| event feed | `feed` | observe stream | perception |
| central | `central` (current focus) | observe focus | working memory |
| right-hand | `scoreboard` | observe stats | proprioception |

- **`feed`** — ordered event log:
  - `run_started { run_id, benchmark, persona, task_count }`
  - `turn { task, text }` (the persona's answer; coalesced, not token-flooded — #170)
  - `task_graded { task, ok, pass_n, done, total, latency_ms, acts }`
  - `run_done { pass_rate, provenance }`
- **`scoreboard`** — latest snapshot:
  `{ run_id, done, total, pass, current_task, provenance: CLEAN|CONTENDED, vram_free }`
- **`central`** — current task + answer-in-flight (a focus field on the progress event).
- **`meta`** — `{ provenance, model, clean_lane, started_at }`.

**Provenance is on the face of it.** "Is this number clean?" is a first-class field
(a green CLEAN / amber CONTENDED chip), never something a viewer infers by watching
who else is inferring. Derived from the eval-preemption quiesce state
(`eval.quiesce` probe, `eval.rs:870`).

## What already exists (build on it, don't reinvent)

- **`eval:progress`** bus event — published at `eval.rs:1786` from `report_task_graded`
  (`eval.rs:1761`), carries `EvalPassProgress { done,total,pass,current_task,last_ok,
  updated_at_ms,vram_free }` (`eval.rs:1722`). **No consumer today** — a ready tap.
- **watch** — `subscribe_eval_progress()` (`eval.rs:1755`), read by `cognition/eval-status`.
- **ledger** — `append_progress_ledger` → `~/.continuum/progress/<persona>.jsonl`
  (per-run rows: pass_rate, note, evalSet, timings). Durable truth.
- **`eval.task` probe** — per-task grades (currently log-only).
- **room emit** — `AircCitizen::say(text)` (`persona/airc_citizen.rs:101`, impl
  `context/airc_adapter.rs:119`) → the room. `chat:posted` → `inbound_attach` →
  `ipc/positron_source::spawn` (`:579`) → Substrate → widget. **Turns/grades as room
  messages render for free in every idiom** — no new payload type, no new renderer.
- **typed room publish** — `airc/realtime-publish` (explicit `room_id`, typed
  payloads) — the seam for the `scoreboard`/presence, later.

## Slices

1. **Agentic `observe` surface (SAFE first — no `eval.rs` cognition change, no
   identity attach).** A command that aggregates the existing taps
   (`eval:progress` watch + ledger + provenance) into the three-subscription model
   above, returned as clean structured state. This *is* the information model made
   real, and it retires operator log-grep — the dogfood. Every future renderer
   (widget, persona) consumes the same shape.
2. **Room projection.** Eval holds a **proctor citizen** attached to the benchmark
   `room` it already carries (`EvalParams.room_id`, `eval.rs:538`); emit one
   `say()` at `report_task_graded` (grades) and one in `eval_settle` (turns, guarded
   to the benchmark room — forks are silent today, `eval.rs:1194`). Now the run is a
   joinable tab. *(Touches `eval.rs` — a STOP-list cognition file — and stands up an
   airc identity; do it carefully, not rushed.)*
3. **Positron widgets (#141).** Feed + scoreboard + CLEAN/CONTENDED chip, rendering
   the same room/`eval:progress` the `observe` skill already consumes. Falls out.

## Gating bug (independent, higher priority than the pretty surface)

The **eval-preemption quiesce did not reliably hold** on 2026-07-17 (personas were
inferring during Asha's hard-rs run → `2/8` is a CONTENDED, suspect number, not a
valid re-baseline vs the Jul-14 `0.375`). No renderer matters if the number is a
lie. Fix/verify the quiesce (or despawn the fleet) so a run is provably CLEAN before
trusting any number — and surface that CLEAN/CONTENDED state as the `provenance`
field above so the honesty is visible, not inferred.
