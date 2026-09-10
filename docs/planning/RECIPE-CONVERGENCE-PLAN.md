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

### S2 — One store, one loader
`recipe/run` resolves through `RecipeExperienceSource` (embedded + overlay) instead of the
`recipes` data collection. Any existing rows are dumped to overlay files by a one-shot
migration, then the collection path is deleted.

- **Acceptance:** `recipe/run <name>` works against an overlay file; the `recipes` collection
  read path no longer exists; `activity/recipes` and `activity/spawn` list and accept the same
  set as before plus any pipeline recipes migrated.

### S3 — A pipeline runs in a room *(card 360143a1)*
`PipelineExecutor` gains room scope: `$room`, `$card`, `$args.*`, and the event edges the work
board already emits (`on: card.claimed`, `on: card.settled`, `on: card.due`). `activity/spawn`
runs the recipe's pipeline after the room exists.

Approval is a step property, not a policy elsewhere: `"approval": "human"` on a step means the
step pauses and asks. Irreversible outward actions are the reason this exists.

- **Acceptance:** an authored recipe with a pipeline of existing verbs spawns a room **and does
  work**, with no Rust committed. A step marked `approval: human` does not run unattended.

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
