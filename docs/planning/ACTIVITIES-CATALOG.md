# ACTIVITIES CATALOG — parameterized recipes and everything we intend to run in them (#433)

**Status:** design + inventory. Written 2026-08-14 per Joel's directive:

> "Benchmark recipes will probably require parameter based functionality, which we
> will need to codify. We will use it again to help refine our recipes for a
> variety of tasks while remaining dynamic, easy to call defaults, but
> parameterized. We will need to basically write down all of our planned
> activities, with a few conceived oddballs we anticipate users might want.
> Benchmarks, chats and our current activities list, many tabs already."

This is that write-down. It rides the spawn-gate arc: #429 (spawn hosts what it
births), #431 (recipe strings validated against the registry), #432 (disk overlay
live in prod + every resumed citizen hosted). The code half of #433 lands with
#430 as ONE recipe-schema evolution: `citizens` (roster-as-recipe-data) and
`params` enter `ExperienceRecipe` together.

Doctrine anchors: recipe = content-type + RULES; room = content; benchmarks are
ADAPTERS into recipes, the ROOM is the runner; benchmarks use OUR citizens —
singular or team, any airc member combination, humans included — never disposable
solvers.

---

## 1. The parameter model

A recipe is authored JSON (embedded floor + `<continuum_root>/recipes` overlay,
live since #432). #433 adds one field:

```jsonc
{
  "purpose": "benchmark/swe",
  "regions": [ /* unchanged */ ],
  "affordances": [ /* unchanged */ ],
  "params": {
    "suite":     { "type": "string",  "default": "swe-bench-lite",
                   "doc": "which task manifest to import (task + oracle only)" },
    "instances": { "type": "string[]", "default": [],
                   "doc": "explicit instance ids; empty = suite order" },
    "team":      { "type": "member[]", "default": [],
                   "doc": "declared member composition; empty = solo, resolved at spawn" },
    "budget":    { "type": "duration", "default": "4h",
                   "doc": "wall-clock ceiling per attempt" }
  }
}
```

Rules, in the order they bind:

1. **Params are DATA in the recipe JSON.** Authoring a new parameterized
   activity is a file, zero code — the same contract the overlay already gives
   purposes and regions. Types are the small closed set the schema validates
   (`string`, `number`, `bool`, `duration`, `string[]`, `member`, `member[]`);
   anything richer is a sign the value belongs in a region's own state, not in
   spawn params.
2. **Every param has a default.** `activity/spawn --name X --recipe chat` with
   zero params must always work — low friction is a stated requirement.
   A param without a default is an authoring error refused at load, same
   fail-loud arm as a malformed recipe file (#432).
3. **Callers pass overrides; the spawn validates them** against the declaration
   the same way #431 validates the recipe string: unknown param name or
   type-mismatched value is a loud, actionable refusal naming the declared set.
4. **Resolved params ride the `RoomRecipeBinding`.** The binding on the wall is
   the room's birth certificate; adding the resolved (post-default) param map
   makes every room self-describing — a citizen, a renderer, or a grader reads
   WHAT this room is parameterized to do from the same ViewState pipe as
   everything else. No side-channel run files (BENCHMARKS-ARE-ADAPTERS law).
5. **`member` resolves through airc identity** — a persona, an agent, or a
   human is one member shape (`airc` PeerId, WHO durable). The teams form is
   just `member[]`: declared composition, repeatable as an experimental control
   (the CooperBench on/off delta, #389, needs exactly this).

## 2. Targeting

Targeting is not benchmark-special; it is the general "which content does this
room bind to" axis, first-class in params:

| Param shape | Meaning | Example |
|---|---|---|
| `suite` | a catalog manifest to draw tasks from (#370) | `swe-bench-lite`, `hard-rs`, `erdos` |
| `instances` | explicit item ids within the suite | `["sympy-24152"]` |
| `team` | declared member composition | `[asha, anwen]`, `[operator, atlas]` |
| `budget` | resource ceiling the governor can read | `4h`, `200k-tokens` |

A zero-arg spawn of `benchmark/swe` is a real, runnable default (suite head,
solo, default budget). That is the "easy to call defaults" requirement made
concrete.

## 3. Catalog — SHIPPED recipes (the embedded floor, today)

| Purpose | What it is | Params (#433 target) | Members |
|---|---|---|---|
| `chat` | the default shared room; messages + roster | *(none needed — the proof that params are optional)* | open |
| `benchmark/hard-rs` | Rust-coder benchmark run room; scoreboard region | `suite`, `instances`, `team`, `budget` | declared team or solo citizen |
| `video-chat` | chat + live primary video stage (LiveKit media plane) | `team` (who is invited) | declared |
| `profile` | a citizen's own page; `save` affordance gated Owner | `subject` (whose profile; default = viewer) | subject + viewers |

## 4. Catalog — CURRENT activities without recipes yet (the "many tabs")

These exist as positron ViewStates/tabs today; each becomes a recipe entry so
spawning one is `activity/spawn`, not bespoke wiring. One truth in Rust, N
renderers — the recipe is where the tab's rules become data.

| Tab / activity | Purpose (proposed) | Params | Notes |
|---|---|---|---|
| Kanban / work board | `board` | `scope` (room \| grid) | operator board ≠ citizens board — scope is the param, not two systems |
| Serving console | `console/serving` | `node` (default local) | #284 SCADA face; cross-grid via `node` (#283) |
| Wall | `wall` | `topic` | pinned durable posts |
| Foundry | `foundry` | `recipe` (ForgeRecipe entity ref) | forge runs get a ROOM, so training is perceivable (#141) |
| Metrics / telemetry | `metrics` | `focus` (loss \| evals \| serving) | #141 widgets as regions |
| Nav / directory | `directory` | `grid` (default own) | #258 one directory per grid |

## 5. Catalog — PLANNED activities

| Activity | Purpose | Params | Members | Why it exists |
|---|---|---|---|---|
| SWE benchmark run | `benchmark/swe` | suite, instances, team, budget | citizens ± humans | the campaign; adapter imports task+oracle only, room is the runner |
| Academy classroom | `academy/class` | `curriculum`, `students`, `teacher` | teacher + students | the flywheel room: salience → curriculum → training pairs (#116, #320) |
| Team pair-coding | `pair` | `task`, `team`, `supervision` (on\|off) | exactly the A/B lever | #389's published delta needs supervision as a PARAM |
| Math lane | `benchmark/erdos` | suite=erdos, instances, team | citizens | #376, gated on SWE satisfaction |
| Live mode | `live` | `team`, `av` (voice\|video) | humans + citizens | #285 hero; video-chat generalized |
| Forge run | `foundry/run` | `forge_recipe`, `base_model` | custodian citizen | ForgeRecipe-as-entity (CLAUDE.md forge template arc) |
| Exam room | `exam` | `suite`, `student`, `disclosed_capacity` | one student + proctor | exams use full grid-backed capacity, DISCLOSED; oracle held out |

## 6. Catalog — ODDBALLS (user-shaped, anticipated)

Kept deliberately small and weird — each is a test that the schema bends without
new code. The first (`kitchen-design`) is already the overlay loader's own test
fixture; the rest should each be authorable as one JSON file.

| Activity | Purpose | Params | Why it stresses the schema |
|---|---|---|---|
| Kitchen design | `kitchen-design` | `canvas_size` | canvas region, zero chat — proves regions carry the weight |
| Book club | `club/book` | `book`, `cadence` | recurring sessions → prospective memory (#125) as recipe RULES |
| D&D campaign | `game/campaign` | `system`, `dm`, `party` | a `member` param with a ROLE (dm) — role-in-team shape |
| Watch party | `party/watch` | `media_url`, `team` | media region + synchronized playback state |
| Homework tutoring | `tutor` | `subject`, `student`, `patience` | a number param that tunes cognition-side pacing policy |
| Music jam | `jam` | `bpm`, `team` | latency-sensitive live media — the video-chat outlier B |

Per the outlier-validation strategy: build `benchmark/swe` (heaviest params) and
ONE oddball (`club/book`, recurring-rules shape) first. If both fit the schema
without forcing, the rest are data entry.

## 7. What #430 + #433 change in code (one schema evolution)

1. `ExperienceRecipe` gains `citizens` (#430: the roster the room wants hosted —
   kills `plan_for_tier`'s hardcoded vec) and `params` (#433: declarations with
   defaults) in the SAME version bump — one migration, not two.
2. `activity/spawn` accepts `--param key=value` (and a JSON map on the command
   surface), validates against the declaration, merges defaults.
3. `RoomRecipeBinding` carries the resolved param map; renderers and citizens
   read it through the existing ViewState pipe.
4. `benchmark/dispatch` (THE one adapter) stops carrying run shape in ledger
   files and passes it as params — the acceptance test stays: *can a citizen
   standing in the room perceive the run's state through the ViewState pipe?*

Out of scope here: recipe-owned ActivityObjective scoring (#371) composes with
params but is its own card; parameterized RECURRENCE (book club cadence) needs
#125's intention substrate and is noted, not designed.
