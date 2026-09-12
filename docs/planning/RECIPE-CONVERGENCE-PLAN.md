# PLAN: one recipe system, then benchmarks onto it

**Status:** plan, ready to execute. Written 2026-09-10 against `3687d5a37`.
Audit this implements: [RECIPES-ARE-THREE-SYSTEMS-THAT-NEVER-MEET](../architecture/RECIPES-ARE-THREE-SYSTEMS-THAT-NEVER-MEET.md).
Cards: c2ec4b96 · 360143a1 · c6648d08 · dbdd6bcf · fa447bfe · d92a5941 · 92de4d78 · 7d8f8fa8

> **Joel's scope call, 2026-09-10 — this is the whole shape of the work:**
>
> *"Other than chat (and live chat), the only real place this has been used exotically is the
> benchmarks. The others are mostly positronic pages like themes and settings, or profile
> stuff. If we can fix the recipe system to not be a cluttered confusing mess, do the SAME for
> benchmarks which MUST call into these same commands, then we are set. The profile pages and
> things like theme/universe are simple (academy is also part of benchmarks really)."*

Two phases, and the second is the proof of the first. **Nothing here is a new subsystem** —
every slice deletes a parallel path or wires an existing one.

---

## 0. What the recipe layer actually has to serve

Naming the real consumers stops this from becoming a framework nobody needs.

| Consumer | What it needs from a recipe | Status after this plan |
|---|---|---|
| Chat, live chat | regions (messages, roster, stage) | works today, must not regress |
| **Profile, theme/universe, settings** | **regions only — they are positron pages** | works today, must not regress |
| **Benchmark (academy included)** | **regions + affordances + pipeline** | the whole job |
| A user's arbitrary ask | the same three, authored not compiled | falls out of the above |

So the schema needs exactly three live capabilities: **regions** (have them), **affordances**
(inert), **pipeline** (absent). Anything beyond those three is scope creep.

The positron pages are the regression suite, not a target. Every slice below must leave them
untouched — if a theme page breaks, the slice is wrong.

## 1. The end state, in one file

One schema, one store, one loader, one executor:

```jsonc
{
  "purpose": "benchmark/swe",
  "base": "academy",
  "regions":     [ /* what the room shows — unchanged, already works */ ],
  "affordances": [ /* what citizens in this room MAY DO — S1 makes this real */ ],
  "citizens":    [ /* roles seated */ ],
  "pipeline":    [ /* what the activity DOES — S3 makes this real */ ],
  "params":      { /* declared defaults that are actually the defaults — S0 */ }
}
```

Loaded from one place (`RecipeExperienceSource`: embedded floor + `<continuum_root>/recipes`
overlay), executed by one engine (`recipe::PipelineExecutor`), entered by `activity/spawn`.

Deleted on the way: the `benchmark_recipes` collection, the `recipes` collection load path,
`benchmark.rs::load_recipe`, its private param interpolation, the hand-built `run_params`, and
the `bench_round` card mirror.

---

## Phase A — make the recipe system one thing

### S0 — One schema, one set of defaults
Merge `recipe::types::Recipe` (the pipeline shape) into `experience::Recipe` as an **optional**
`pipeline` field. One struct, one serde shape, one validator. Nothing executes it yet.

- *What this catches:* an authored activity that DOES something must round-trip through the
  authoring path (`ExperienceRecipe::from_json`); a page recipe with no pipeline must stay a
  page.
- **Acceptance:** all five shipped recipes load unchanged and declare no pipeline; a recipe
  carrying a pipeline round-trips; the ts-rs bindings regenerate without hand-editing.

**Moved to S4 — the defaults lie.** `benchmark.json` declares `driver.default: "citizen"` while
`benchmark.rs:1906` uses `p.drive.unwrap_or_default()` (the Rust `Default` of the CLI arg
type). Fixing it means dispatch must resolve params *through* the recipe, which is exactly what
S4 does — doing it here would build that resolution twice. The defect is real and stays owned;
it is repaired where the recipe becomes dispatch's source of truth, not before.

**Landed 2026-09-10:** `pipeline: Vec<RecipeStep>` on `ExperienceRecipe`, reusing
`recipe::types::RecipeStep` (one step shape for both entry points, now ts-rs exported).
58 experience tests, 134 recipe tests, 19 activity tests green.

### S1 — Affordances select the tool surface *(card c2ec4b96 — the load-bearing slice)*
`hands_surface()` becomes room-aware. The room's Experience manifest supplies the surface:

- affordances **non-empty** → that set is the surface for citizens acting in that room
- affordances **empty** → today's prefix filter, unchanged (every existing room keeps working)

One filter, one place — `hands_surface` stays the only site, it just gains the room as an
input. No second clamp anywhere (`[[filter-once-centrally...]]`).

- *What this catches:* a recipe that says a room may use `web/fetch` and nothing else, and a
  citizen in that room who is offered exactly that.
- **Acceptance:** a room whose recipe declares affordances offers those verbs and only those;
  a room declaring none is byte-identical to today's surface; the benchmark rooms' surface is
  unchanged until S4 authors their affordances.
- **Risk:** this is the slice that can silently amputate a citizen's hands. The empty-set
  fallback is the guard, and the test asserts it explicitly.

**Landed 2026-09-11.** The room's manifest now reaches the mind: ipc installs the node's one
`RecipeExperienceSource` (`experience::source::install_node_experience_source`, the same
registry the positron projection resolves from), the persona spawn passes it into
`PersonaBrainConfig.experience`, the cycle stamps `Workspace.room_affordances` on every tick,
and `select_tool_surface` takes the room's set first — before focus and budget, because both
narrow a surface the room has already bounded. Keyed on RAW command names cached beside the
wire-dialect specs (the 2026-09-04 muting was a name-space mismatch at exactly this seam).
Three guards, each a test: a room declaring nothing is byte-identical to pre-S1 on every arm;
a room whose affordances match no native verb falls back to the pre-S1 surface and probes
`delib.surface.room_affordances_unmatched` — never an empty offer; and the shipped benchmark
recipe must authorize every hand a work turn offered, which is how `code/list` and
`code/tree` got into `benchmark.json` (the test named them; I had not known they were native).

**The one deliberate behaviour change:** `benchmark.json` now authors its affordances — her
hands, `web/fetch` + `web/search` (native as a direct task-score lever), `room/members`, and
`cognition/observe`. Work turns in a benchmark room are unchanged. MESSAGE turns there no
longer offer `activity/spawn|invite|recipes`, `interface/screenshot`, `perception/look`,
`vision/look`: a solver's room withholds what a solver does not need. That is the rule doing
its job, and it is fewer tokens per turn; watch `delib.surface` reasons after the deploy.
Faculty 70/70, workspace 27, experience 55, persona_workspace 21, activity 19 green.

### S2 — One store, one loader
`recipe/run` resolves through `RecipeExperienceSource` (embedded + overlay) instead of the
`recipes` data collection. Any existing rows are dumped to overlay files by a one-shot
migration, then the collection path is deleted.

- **Acceptance:** `recipe/run <name>` works against an overlay file; the `recipes` collection
  read path no longer exists; `activity/recipes` and `activity/spawn` list and accept the same
  set as before plus any pipeline recipes migrated.

**Landed 2026-09-11.** No migration was needed: the `recipes` data collection held ZERO rows on
this node — the second store never had a tenant. `ExperienceSource` gains `recipe_for_purpose`;
`recipe/run` resolves its name as a purpose through the node's one source (the same catalogue
`activity/recipes` lists and `activity/spawn` accepts); `PipelineExecutor::run` takes
`(name, &[RecipeStep], args)`; `recipe::types::Recipe` is deleted, its tolerant-parse
invariant folded into the S0 test. Net: one type, one loader and one store path removed.
RECIPE-EXECUTION-RUNTIME.md's "Violation 5" (collection vs files overlap) is closed by it.
recipe 75, experience 59, recipe_run 2, generate_recipe 46 green.

### S3 — A pipeline runs in a room *(card 360143a1)*
`PipelineExecutor` gains room scope: `$room`, `$card`, `$args.*`, and the event edges the work
board already emits (`on: card.claimed`, `on: card.settled`, `on: card.due`). `activity/spawn`
runs the recipe's pipeline after the room exists.

Approval is a step property, not a policy elsewhere: `"approval": "human"` on a step means the
step pauses and asks. Irreversible outward actions are the reason this exists.

- **Acceptance:** an authored recipe with a pipeline of existing verbs spawns a room **and does
  work**, with no Rust committed. A step marked `approval: human` does not run unattended.

**Landed 2026-09-11 (S3, the first half).** The ONE birth path — `spawn_activity_room`, all five
callers — runs the recipe's pipeline after the room is bound, with `$room.{id,name,recipe}`
seeded and the resolved params as `$args`; the spawn result carries the receipt
(`pipeline.{steps_run,steps_skipped,held_at,trace}`). `approval: "human"` on a step HOLDS the
run there — not dispatched, receipt names it, nothing after it runs; proven over an empty
registry where any dispatch would have failed loudly. Callers without an executor at hand
(three of the five today) probe `activity.pipeline.unrun` for a recipe that declares one —
a wiring fact, never a silent no-op. `recipe/run` reports `held_at` too.

**S3b — still owed:** the event edges (`on: card.claimed | card.settled | card.due`) and
fan-out (`each`). Both need a subscriber per spawned room that re-enters the pipeline at the
edge with `$event` bound; the board already emits `work.card.state_changed`. S4's benchmark
pipeline needs `each` (one card per task) and `on: card.settled` (grade), so S3b lands
inside S4 rather than as a framework built ahead of its one consumer.

### S3c — The schema slice: recipes as an API, typed and validated *(Joel: "I love API schemas… UUIDs and structures… strict Rust")*
Landed 2026-09-11. Three things, all off every hot path (once per spawn, once per catalogue
read; the registry lookup is built once per process):

- **Typed step vocabulary.** `on_error: OnError {Fail (default), Skip}` and `approval:
  Option<Approval {Human}>` are enums, not `Option<String>`: a misspelled policy is refused at
  load and named, never silently the default. Unknown FIELDS stay tolerated (a newer file loads
  on an older executor); unknown VALUES are refused. Both exported to TS and JSON Schema.
- **A published recipe schema.** `protocol/schema/experience-recipe.schema.json`, generated from
  `ExperienceRecipe` (schemars) on every test run exactly as the ts-rs bindings are, and gated
  by the same drift job (now diffs `protocol/schema/` too). An authored file carries
  `"$schema"` and an editor validates and completes it. This is what makes **complete
  customization without repo changes** safe: the overlay directory is the store, the schema is
  the contract, and the gate below names what is wrong instead of asking for a compiler.
- **Definitive pipeline validation** (`recipe/validate.rs`, `pipeline_issues`): every step
  checked against the COMMAND REGISTRY'S OWN param schemas — command exists; literal values
  typed by `type`/`enum` through `$ref` definitions; every `$reference` RESOLVED (`$args.x` a
  declared param, typed by its default; `$room.*` a seeded field, asserted against the seeding
  site; `$item`/`$index` only inside `each`; anything else an earlier step's `outputTo`).
  `activity/recipes` lists issues per recipe (typed `PipelineIssue`, ts-rs); `activity/spawn`
  refuses a recipe with any. First run against the live registry caught a real hole in my own
  `benchmark/round-track` (skipped schema fields) — the gate paid for itself immediately.

**Not yet definitive:** the SHAPE of a prior step's output (`$imported.cards` is known to
exist, not what it holds). Command outputs carry a TS type, not a JSON Schema; the next step is
`Output: JsonSchema` on `ActionCommand` so `$item.title` is typed too.

## Phase B — benchmarks onto it

### S4 — Benchmark becomes an authored recipe *(card fa447bfe)*
Author `benchmark/swe` with affordances (`code/*`, `work/*`, `cognition/observe`) and a
pipeline: `benchmark/import` → `work/create` per task → `activity/invite` → `work/grade` on
`card.settled`. Section 5 of the audit doc is the file.

Run it **beside** the Rust path on the same seed, and assert the same board, the same card
rooms and the same verdicts. Only then delete `benchmark_recipes`, `load_recipe`, the private
interpolation, the hand-built `run_params`, and the `bench_round` card mirror.

- **Acceptance:** a seeded round dispatched from the recipe matches the Rust path's board and
  verdicts; the deleted-code diff is net negative; academy rounds ride the same recipe.
- **Note:** this subsumes the card-state-is-the-board work already planned — the board is the
  only card store once the mirror is gone.

**Landed 2026-09-11 (S4a — the round is authored; dispatch stands beside it).**
- `prepare_cards(spec, selection)` is extracted from dispatch as THE ONE WRITER of benchmark
  cards (task + oracle only, no side effects; the env pre-warm stays a dispatch side effect
  in `prewarm_swe_envs`). `CardWork`/`PreparedCard` lifted to module scope. Dispatch calls it;
  so does the new verb. Identical cards by construction, not by comparison.
- Three recipe-facing verbs (`commands/benchmark_import.rs`): `benchmark/import` (pure, rows
  carry the writer's title/body + the parser's task id), and two DELIBERATELY TEMPORARY tracker
  adapters `benchmark/round-open` and `benchmark/round-track` — they exist because the tracker
  still holds a second copy of card state; when the board-projection work deletes the tracker,
  both verbs go and the pipeline gets two steps shorter.
- `each` fan-out in the executor (`$item`/`$index` per element, results bound as an array;
  proven over an empty registry: three items → three dispatches, never one with the array).
- `benchmark-swe.json` (`benchmark/swe`, base academy, the same affordances as the Rust door):
  import → round-open → `work/create` ×each → round-track → invite → doctrine. Params resolve
  through the recipe's DECLARED defaults (`driver: citizen` is now real — the S0-deferred lie
  is closed on this path). `activity/spawn` routes a benchmark purpose to dispatch ONLY when the
  recipe declares no pipeline; a recipe that declares its behaviour drives itself.
- Floor tests: every page recipe declares no pipeline; the authored round is made only of a
  known verb set (a new step = a new verb, on purpose); both benchmark recipes authorize every
  hand a work turn offers.

**S4b, first live run (2026-09-11 23:4xZ, M5 on c5b566a58):** `activity/spawn --recipe
benchmark/swe` (seed 2, sample 3) → room 5e1754ec, the six-step pipeline ran with no Rust in the
path (import → round-open → `work/create` ×3 → round-track → invite; doctrine skipped by its
condition); `benchmark/dispatch` on the same seed drew the SAME three instances (deterministic) but
its already-resolved gate skipped two → the gate lived inside dispatch's `run()`, not in import.
Ported: `benchmark/import` gains `skipAlreadyResolved` (default true, receipt names the skipped)
and `limit` (the gym suites do not sample; dispatch's cap, kept). Then `suite: hard-rs` through the
same recipe → room 70fab522, 8 gym cards — every suite, one recipe. The recipe is renamed
**`benchmark/round`** (`suite` is a param; `benchmark/swe` was never the right name).

**Still owed for S4 (S4b, remaining):** the side-by-side run itself — `activity/spawn --recipe
benchmark/swe` and `benchmark/dispatch` on the same `(suite, seed, sample)` after a deploy,
asserting the same board, card rooms and verdicts (a live acceptance, not a unit test); kickoff
parity (dispatch's addressed per-card kickoff vs the recipe's one doctrine line — decide which
is the design, the one-deck-kickoff rule says the recipe's); detached-solve pre-claim +
staging as a step; then delete `benchmark/dispatch`, `benchmark_recipes`, `load_recipe`, the
hand-built `run_params`, and — with the board as the card store — the tracker and its two
adapter verbs. The `on:` event edge is not needed for the round: grading already fires from
`work.card.state_changed` in `modules::benchmark_grade`, keyed on the card title.

### S5 — An ask becomes a recipe *(cards c6648d08, dbdd6bcf, 7d8f8fa8)*
Register `cognition/generate-recipe` (its prompt, parser, validator and orchestrator are
already written and unreachable), give it a write path to the overlay dir, and let a citizen
who hears an ask in `#general` or a DM compose → show the human → spawn on approval.

Placed last **because the generator must emit the settled schema.** Wiring it before S0–S3
means teaching it a shape we are about to change.

- **Acceptance:** a plain-language ask in a room produces a recipe file, a room, and a board,
  with one human approval in between.

### S6 — Close the holes that let this happen
- `layout` lives or dies *(card d92a5941)* — either a spawned room arranges regions per its
  recipe, or the field leaves the schema and the shipped files.
- The ratchet *(card 92de4d78)*: a test asserting every field the schema declares has at least
  one shipped recipe exercising it **and** at least one consumer reading it. Same shape as the
  eviction-decision test — it fails on an undecided field. This is what prevents a fourth
  recipe system.

---

## Phase C — non-repo extensibility: bring your own commands *(Joel, 2026-09-11)*

> *"Streamline this and the recipe system for non-repo owned extensibility while allowing for
> patterns as sophisticated as our benchmarks to run, which of course work from commands.
> Perhaps they'll link their own commands, share them or call other processes. Maybe it's
> just scriptable, while offering performance."*

The recipe layer is data (S0–S4a) and gated by schemas (S3c), but a pipeline could only name
verbs that are **Rust in this repo**. The escape hatch existed — `code/shell` / `code/run`
execute any process — but a user's script was not a *verb*: no name, no schema, not listable,
not affordance-able, invisible to the gate. So: **a command is a manifest.**

### S7 — Process commands from a manifest directory
- **Manifest** (`<continuum_root>/commands/<name>.json`, the same overlay law as recipes;
  published schema `protocol/schema/command-manifest.schema.json`): `name`, `description`,
  `access` (`ai_safe` | `privileged`), `native`, `params` (JSON Schema), `exec` (argv), `cwd`,
  `wire` (`json_stdio` now; `socket` declared, refused until built), `timeout_ms`.
- **Runtime:** `sdk_codegen::ext::ProcessCommand: DynCommand` — spawn `exec`, params as JSON on
  stdin, JSON on stdout is the result, non-zero exit is a named error, `timeout_ms` is the bound
  (probes `ext.command.{spawned,finished,failed,timed_out}`). `modules/ext_commands.rs` loads
  the directory at boot and hands the objects to the kernel through `ServiceModule::commands()`
  — no parallel router.
- **Catalogue:** `command_registry_live()` = the compile-time inventory ∪ the manifests'
  descriptors; a manifest that shadows a built-in is refused by name. `commands/list`,
  `commands/help`, `native_tool_specs`, `tool_dialect`, the ACL's command sets and the recipe
  gate's `registry_lookup` all read it — an authored verb is indistinguishable from a shipped
  one to a recipe, a citizen, and the validator. Codegen alone keeps the static list: a
  manifest is content, not source.
- **Sharing:** a manifest is a file and travels as a recipe does; a relative `exec` resolves
  beside the manifest so a shared directory carries its scripts. A recipe naming a verb this
  node lacks is refused at the door with the verb named (S3c) — capability is checked where the
  work would run.
- **Performance:** one bounded spawn per call, never on a turn's hot path unless a room's
  recipe offers the verb as an affordance — then it is one act, priced like any other.
- **Acceptance:** drop a manifest, reboot → `commands/list` shows it with its schema → a recipe
  step naming it validates → `activity/spawn` runs it → a citizen in a room whose recipe offers
  it can call it; a colliding or malformed manifest is refused by file and reason, never
  silently.

### S8 — Benchmarks as a shipped *example* of Phase C
`benchmark/import` and the gym adapters become manifests + recipes a user could have written;
the repo ships them as the worked example, not the only way. The test that Phase C is real:
the most sophisticated pattern we run must be expressible without this repository.

## Order, and why

```
S0 schema ─┬─ S1 affordances ──┐
           └─ S2 one store ────┼─ S3 pipeline in a room ── S4 benchmark ── S5 ask→recipe ── S6 ratchet
                               ┘
```

S1 before S3: a pipeline's steps run as citizens whose capabilities the affordances define;
wiring execution first means running with the global surface and re-doing it. S4 after S3 for
the obvious reason, and S5 after S4 so the generator learns a schema that has been proven by a
real consumer rather than an imagined one.

## Standing rules for this work

Deploy via `continuum reboot` + SHA verify. Probes, not `tracing`. Reuse
`RecipeExperienceSource`, `PipelineExecutor`, `hands_surface`, the work board — never a
parallel primitive; adding one is the exact defect being repaired. One test mod per file, each
test carrying a `// what this catches:` line. Feature-branch commits are free; the merge to
canary takes a peer's no-objection and green CI.

**The check on every slice:** does it delete more than it adds? Phase A should be roughly
neutral. S4 must be strongly negative. If a slice grows the tree, it is building a framework
instead of removing a parallel path.
