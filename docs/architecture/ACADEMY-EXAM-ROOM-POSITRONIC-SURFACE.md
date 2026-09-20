# Academy Exam-Room Positronic Surface

**The walk-in doctrine (Joel, 2026-08-08):** *"Build all your harnesses first, feedback for positron, first. Only way to be able to iterate quickly and accurately. Remember what I said about being able to walk into the exam room (will do this literally in 3D positron and ui) to assist them, monitor, improve, direct, teach."*

This document is the design-before-build for that surface: what every consumer — human, teacher persona, solver persona, operator agent — needs to SEE and DO in a benchmark run, and the data/events layer that feeds all of them from ONE projection. The agent-side data/events ship first; every widget is a render of something an agent can already query or subscribe to. Positronic parity is the acceptance test: if the human widget can show it, the persona can perceive it and the agent can `jq` it — same projection, different window.

Grounding docs: [POSITRON define-once contract](../positron/), #329 (benchmark IS a live room), #274 (recipe=content-type, room=content), #346 (grading sentinel), #184 (persona tile = live glass box), the harnesses-first ruling. Live evidence that motivated this: 2026-08-08, both graded solves stalled silently for 2.5h and the operator found out by *asking* — the run had no pulse anyone could see.

---

## 1. The room tree

```
academy/                      ← commons (citizens land here; teachers live here)
academy/bench/<run_id>        ← ONE room per graded run (recipe: bench-run)
```

- A run **is** a room (#329). Spawned by the dispatcher via `activity/spawn` with the `bench-run` recipe at solve launch; the solver, the grader's announcements, and anyone who walks in share it.
- Rooms never auto-delete — a finished run's room IS the evidence trail; it archives (#274).
- The solver is `room/join`ed to it for the run's duration (her turn-room perception, #2175, makes her turns read THIS room). Teachers and humans subscribe at will — that's the walk-in.

## 2. What each consumer needs

| Consumer | Sees | Does |
|---|---|---|
| **Solver persona** | Task framing, her own act receipts, verdicts, deadline pulse (her own clock), trail | Works; asks for help in-room (speech is in the same transcript) |
| **Teacher persona** | Pulse, act ledger, grade history, her stuck-signatures | Speaks coaching into her next turn (TEACH-during-exam); never touches the workspace |
| **Human (Joel)** | Everything below, rendered | Walks in, talks mid-run, pauses/grants/regrades, later in literal 3D |
| **Operator agent (me/BigMama)** | `benchmark/runs` projection + probe stream | Monitors cadence, relaunches, glass-boxes failures — same events, no bespoke bash |

Same data everywhere. Humans add render + intervention affordances; agents add queryability + subscription; personas receive it as perception. One projection.

## 3. Tabs (per bench room)

1. **Live** *(default)* — the run's transcript: task framing card, one collapsed card per act (`#n tool(args) → result summary`, expandable to full receipt, #243 rendering), a verdict card per attempt (F2P/P2P, failing tests, patch bytes), and every spoken message — solver, teacher, human — interleaved in time. Walking in = this tab + typing.
2. **Workspace** — her working tree: `files_examined` trail (first-touch order, the #2177 state), diff-so-far (`workspace_patch`), file viewer. Read-only for everyone but her.
3. **Mind** *(glass box, #184)* — per-tick cognition: faculty bids/salience, the decision, window budget accounting (#327), recall deliveries. The capture-sink stream, rendered.
4. **Scorecard** — attempts × grades matrix, act budget spent per attempt, wall-clock per attempt, patch-size trajectory. The falsifiable summary a README claim cites.

## 4. Right-rail widgets

Each widget names the exact event classes it folds — that IS its API contract; a widget with no named feed doesn't get built.

| Widget | Shows | Feeds (existing unless marked) |
|---|---|---|
| **Run pulse** | attempt N/M, phase (staging → solving → grading → retry), last-tick age, per-attempt deadline countdown; RED on stall | `benchmark.attempt.start` (#2180), capture tick cadence, `benchmark.stall` (#2180), `benchmark.autograde` |
| **Act ledger** | rolling `[action #n]` chips with ok/error state | act receipts (capture stream) |
| **Grade history** | F2P/P2P sparkline per attempt, patch bytes | `benchmark.autograde` |
| **Budget gauges** | acts used/max, served window vs demand, attempt clock | solve params, `serving.plan`, pulse |
| **Serving health** | lane residency, tok/s, generation-verified liveness (#363) | `serving.plan`, lane probes |
| **Trail** | files her acts named, first-touch order | `files_examined` (#2177) |
| **Presence** | who is in the room now | room roster |
| **Intervene** *(human + teacher)* | speak-into-next-turn, pause/resume, grant +N acts, trigger regrade | NEW commands (§6), consent-gated (#136), every use lands in the transcript |

## 5. Data & events — the agent side, build order

The rule: projection first, render second. Every layer below is consumable by an agent before any pixel exists.

1. **Pulse + stall probes** — SHIPPED (#2180): `benchmark.attempt.start` per attempt, `benchmark.stall` on deadline expiry, alongside existing `benchmark.autograde`. A run now has a heartbeat and a loud death.
2. **RunProjection: `benchmark/runs`** — NEXT (this doc's first build): one query returning `RunCard`s folded from what already exists on disk — progress files (`agent-solve-<run>.json` / `.grade.json`) + capture-sink mtime/tick-count. Fields: `run_id, instance, solver, phase, attempt/max, acts, last_activity_ms, stalled, grade{resolved,f2p,p2p,patch_bytes,failed_tests}, files_examined`. Poll-shape v1; watch-snapshot v2 on the ModelCatalog pattern (#78). This is the tab bar's data source and my Monitor's query in one.
3. **Per-run bench room** — `bench-run` recipe + solve announcing into it: attempt starts, verdicts, stalls as room messages (progress as chat). #274 tail / #346 slice 3. After this, "walking in" already works in the existing chat UI with zero new widgets.
4. **Probe→room bridge** — `benchmark.*` classes for a run also emit onto that run's room event stream (probe router, #362), so ANY subscriber — widget, teacher persona, my Monitor — gets push, not poll.
5. **Widgets** — positron lane consumes RunCard + room stream (#141/#184 patterns). 3D positron renders the same room later; no new data work.
6. **Intervention verbs** — `run/pause`, `run/grant-acts`, `run/regrade`, consent-gated (#136), transcript-recorded. Last, because observation must be trustworthy before control is.

## 6. What this deliberately is NOT

- Not a parallel telemetry system — every feed is an existing probe class or room event; the projection folds, never re-measures.
- Not benchmark-only — the same room/tabs/widgets shape serves any long activity (a training run, a forge job, a real project card). Bench is the outlier-A that proves the recipe.
- Not a steer on the solver — she perceives the same truth everyone sees; intervention is speech and explicit granted state, never hidden nudges.
