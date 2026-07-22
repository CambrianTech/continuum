# Learning-Visibility Widgets — the glass box becomes the HUD

**Status:** design (Joel, 2026-07-22: "write this down. Once we get them coding we are
going to focus here"). Sequencing: AFTER the hands are reliable (the agent/solve
benchmark arc), this is the next focus. Companion to
[BRAIN-HUD-DESIGN.md](BRAIN-HUD-DESIGN.md) / [HUD-MICROWIDGET-ARCHITECTURE.md](HUD-MICROWIDGET-ARCHITECTURE.md);
the thesis is [[iterative-glass-box-methodology-is-the-contribution]] +
[[academy-learning-is-the-show-spectator-entertainment-thesis]]: the SAME primitives an
engineer used by hand to glass-box the mind become ergonomic surfaces, so any person or
persona can watch — and tune — a mind learning. The glass box is the product, not just
the debugger.

**The provenance that makes this real:** every widget below was performed MANUALLY
during the 2026-07-22 benchmark-reliability session (agent/solve battery → contamination
→ kernel panic → lane pool → three emission-idiom fixes). Nothing here needs a new
telemetry system — each widget is a skin over a data source that already exists in tree.

---

## 1. Pass-rate sparkline (persona tile)

Her benchmark score over time, one point per battery run, annotated where a fix,
genome page-in, or dream/decay tick landed. "Atlas: 1/5 Monday → 4/5 today" as a live
HUD element on the persona tile ([[persona-tile-is-a-live-game-hud]]). This is the
learning curve, literally — the falsifiable "she is improving" number
([[genome-loop-first-positive-lift]]).

- **Data source (exists):** `~/.continuum/progress/agent-solve-<run_id>.json` ledgers
  (acts, patch, files_changed, failed/error), `eval:progress` / `agent:solve:complete`
  bus events, `CognitionEvalResult` (pass_rate, per-task rows, latency aggregates).
- **Render rule (non-negotiable):** INFRA rows (lane refused, pressure defer, serving
  wedge) are visually DISTINCT from FAIL rows. The 2026-07-22 lesson: lane thrash read
  as "the whole roster got dumber" until the error strings were read. A widget that
  conflates machine noise with mind performance actively misleads
  ([[proctored-exam-session-dependable-benchmark]] — infra-unavailable is VOID, not 0).

## 2. Idiom ledger — "how canonical are her hands"

Tool-call fluency over time: canonical vs alias vs miss counts per persona/model, misses
shrinking as the genome learns canonical emission. Each miss row links to the capture
that becomes a training pair — the widget IS the corpus-mining view
([[tool-defect-ledger-mined-from-captures]], [[weakness-becomes-a-generator-in-the-library]]).

- **Data source (exists):** `cognition/tool_dialect.rs` already TALLIES every resolution
  (`from_wire_name` → `record(wire, outcome)` → `cognition::tool_usage` snapshot).
  Parser format-id telemetry (`ToolCallFormat::id`) says WHICH idiom lifted each call.
- **The story it tells:** the 2026-07-22 session met three live idioms (hash-commented
  fence calls, fake `[Action #N]` transcripts, assignment-bound script calls) in the
  adapter. The genome's job is to make those rows go quiet; this widget is where you
  watch that happen.

## 3. Hippocampus gauge

Engram counts by kind (Episodic / Semantic / SelfReflection), salience histogram,
decay-drain rate per dream tick, last consolidation time. The "set in her ways" story
as a live dial: 765 stale Semantic consolidations glowing, then fading as the forgetting
drain runs (#221). Slice-2 supersession events (a new fact evicting a contradicted one)
render as replacements, not just decay.

- **Data source (exists):** per-persona `engrams.sqlite` (`engrams` +
  `engram_recall_metadata`), `RecallMetadataRegistry` (salience, access_count,
  last_decayed_ms, protected_until_ms), `hippocampus.decay` probes from
  `DreamConsolidationRegion`.

## 4. Act-vs-narrate meter

Per turn: intents that REACHED HER HANDS (lifted + executed, receipt observed) vs
stayed speech — plus a confab counter (fabricated receipts caught: `Result:` lines with
no matching execution). [[execute-dont-narrate]] as a needle. On 2026-07-22 this needle
moved from "mostly speech, fake zeros on tasks she'd already solved in her head" to
"mostly hands" across three parser commits — that movement is exactly what an operator
(or the persona herself) should be able to SEE.

- **Data source (exists):** act_observe probes (`persona.turn.*`, `apply_act`),
  workspace capture JSONL (`cognition/workspace_capture.rs` — bids, DECISION, timings),
  prompt-captures, parser format-id.

## 5. Flywheel strip

The L1–L3 loop as a pipeline view: corpus pairs accumulated → training run state →
eval lift per gene → paged-in where. "What she's studying tonight and what it bought
her" ([[coordination-learning-flywheel]], [[self-improvement-is-a-control-loop]]).
This is also the autonomic-first surface: it starts read-only (watch the autonomic
loop), and only later admits controls ([[continuous-learning-autonomic-first-then-admit-controls-and-state]]).

- **Data source (exists):** `dataset/from-turns` outputs, forge/mlx job events
  (`spawn_train_job` bus events), trained-adapter manifest, `cognition/eval` lift
  results, genome paging events.

## 6. Machine-vs-mind timeline

Pressure level, lane spawns/refusals/defers (`eval.lane.pressure_defer` probes, the
memory veto), serving lifecycle, and battery events on ONE shared timeline — so a dip
in scores visibly coincides with a build spike or a lane wedge instead of being read as
regression. The kernel-panic postmortem (2026-07-22) as a permanent instrument.

- **Data source (exists):** `MemoryPressureMonitor` watch channel (`PressureSnapshot`),
  placement-decision JSONL (`placement_capture`), lane registry records, probe stream.

---

## Shell placement — WHO / WHAT / WHICH (Joel, 2026-07-22)

The app shell's three panels have pinned semantics, and the widgets above map onto
them:

| Panel | Semantic | What lives there |
|---|---|---|
| **Left** | **WHO** — global scope | The citizens: roster, personas, presence. The persona tile (with its pass-rate sparkline and act-vs-narrate needle) lives here — learning state is part of who she IS. |
| **Center** | **WHAT** — the main concern | The activity itself: the chat, the code, the benchmark board, the model card. Widget #1's full learning-curve view opens here when learning IS the activity (the Academy-as-show surface). |
| **Right** | **WHICH** — the many | Contextual pickers, searches, and side-conversations supporting the WHAT: Hugging Face model searches, chats with helper AIs, candidate genes, alternate baselines. The idiom ledger, hippocampus gauge, flywheel strip, and machine-vs-mind timeline stack here per activity — you choose WHICH lens feeds the work. |

The right panel is not a passive inspector — it is the selection/support surface, and
MANY widgets stack in it per activity. Mobile keeps its own central-focused UX (the
WHAT), with WHO and WHICH behind drawers — never a squished three-pane.

## Principles (carried from the session that birthed this doc)

1. **Receipts, not vibes.** Every widget cell links to its receipt — the ledger, the
   capture line, the sqlite row. A number you can't click through to its mechanism is
   a number you'll misread ([[reliability-is-it-works-not-that-it-reports-failure-well]]).
2. **INFRA ≠ FAIL.** Machine noise and mind performance never share a color.
3. **Autonomic first.** Read-only surfaces over the ALREADY-RUNNING loops; controls
   come after trust ([[continuous-learning-autonomic-first-then-admit-controls-and-state]]).
4. **Personas are viewers too.** These are not operator-only dashboards — a persona
   inspecting her own idiom ledger / hippocampus gauge is a citizen doing metacognition,
   and eventually tuning herself ([[ask-the-personas-about-their-own-confusion]],
   "laymen AND personas become ML engineers").
5. **No new telemetry for v1.** Every widget above skins an existing source. If a v1
   widget needs a new collector, it's the wrong v1 widget.
