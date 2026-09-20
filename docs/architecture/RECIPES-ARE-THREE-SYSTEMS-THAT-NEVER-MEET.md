# Recipes are THREE systems that never meet

**Status:** audit + the fix. Written 2026-09-10 against build 5193 / `335cb25e4`, after Joel
asked whether an ordinary user could ask a persona in `#general` for an activity nobody coded
— a job-application campaign — and get it. They cannot, and the reason is not the ask.

Companion: [AN-ASK-NOBODY-CODED-IS-A-RECIPE](../activities/AN-ASK-NOBODY-CODED-IS-A-RECIPE.md)
(why the long tail of user asks is the product). Precedent this doc extends:
[RECIPE-EXECUTION-RUNTIME](RECIPE-EXECUTION-RUNTIME.md),
[BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md).

> **The finding in one line:** we built three separate things called "recipe" — one that
> describes a room but cannot act, one that acts but has no room, and one private to
> benchmarks that does both badly. Each is individually defensible. Together they mean an
> authored recipe cannot express a working activity, which is the entire premise.

---

## 1. The three systems

| | **Activity recipe** | **Pipeline recipe** | **Benchmark recipe** |
|---|---|---|---|
| Code | `experience/recipe.rs` | `recipe/` (`executor`, `state`, `interpolate`, `condition`) | `commands/benchmark.rs:1287` |
| Stored as | JSON files: embedded floor + `<continuum_root>/recipes` overlay | rows in the **`recipes`** data collection | rows in the **`benchmark_recipes`** data collection |
| Entered by | `activity/spawn` | `recipe/run` | `benchmark/dispatch` |
| Declares | purpose, regions, affordances, citizens, params, layout | `pipeline[]` of command invocations, `$var` interpolation, output binding, skip conditions | its own `BenchmarkRecipe` struct + its own param interpolation |
| Can create a room | **yes** | no | yes (via dispatch) |
| Can perform work | **no** | **yes** | yes, in Rust |
| Can grant capabilities | no (see §3) | inherits the caller's | n/a |

Three stores. Three loaders. Three parameter models. None of them is the recipe system.

The cruelty of it: **`recipe/mod.rs` already states the correct doctrine**, verbatim —

> *"Recipes are data. Commands are kernel-level capabilities. A recipe is a row: a `pipeline[]`
> of command invocations... Adding a new recipe — or a new recipe CONCEPT — is authoring data,
> never committing Rust. The extension surface is the command system itself... (the
> static-plumbing trap this module replaces — `reviewers: N` on the benchmark recipe format
> needed a recompile; a pipeline step calling `persona/roster` does not)."*

That module is right, it is built, and **`benchmark.rs` did not use it** — it hand-rolled a
third loader against a fourth store six months later. The trap the module was written to
replace is still in the tree, in the exact subsystem named in the comment.

## 2. What an activity recipe declares that nothing reads

Verified by grep against the whole core, 2026-09-10:

| Field | Status | Evidence |
|---|---|---|
| `regions` | **live** | resolved into the Experience manifest, rendered |
| `citizens` | **live-ish** | `source.rs:256` maps to roles |
| `params` | **partly** | see §4 — declared defaults are not the defaults |
| `affordances` | **INERT** | carried to the manifest (`source.rs:472,524`), then ignored. §3 |
| `layout` | **DEAD** | `benchmark.json` ships a full layout tree; zero consumers in core |
| `pipeline` | **absent** | the field does not exist. An activity recipe cannot express behaviour at all |

An author reads the schema, writes `affordances` and `layout`, and believes their activity is
configured. Nothing tells them otherwise. This is the same class as an unlisted verb — the
author's model and the system disagree in silence.

## 3. Affordances grant nothing — the load-bearing failure

A recipe declares `affordances: [{ "verb": "observe", "command": "cognition/observe" }]`. The
verb exists (`commands/cognition/observe.rs:288`). The affordance rides into the Experience
manifest. And then it plays **no part** in what any citizen can do.

The tool surface is built from a **global** list, filtered by **hardcoded name prefixes**:

```rust
// cognition/llm_deliberation_faculty.rs:413
let raw = persona_tools::native_tool_specs();     // every command on the node
self.hands_specs = hands_surface(&raw)            // :3205 — matches on "code/", "work/", "git/"
```

The room does not enter that decision. The recipe does not enter that decision. Therefore:

- **No activity can bind an integration or plugin to itself.** A campaign room cannot be given
  a mail verb; a forge room cannot be given the foundry verbs and denied the rest.
- **No activity can withhold a capability.** Every citizen everywhere gets the same prefix-filtered
  global surface.
- **"Recipe = content-type + RULES" is half-true.** The content-type is real; the rules are
  decoration.

This is why an infinite recipe space is currently a *layout language*. You can describe any
room you like; every room can do exactly the same things.

## 4. Declared defaults are not the defaults

`benchmark.json` declares `driver.default = "citizen"`. Dispatch does:

```rust
// commands/benchmark.rs:1906
run_params.insert("driver", json!(p.drive.unwrap_or_default()));
// comment claims: "no driver named = the recipe default (citizen)"
```

`unwrap_or_default()` is the Rust `Default` impl of the CLI argument type. **The recipe's
declared default is never read.** They agree today by coincidence. Two sources of truth for
one value, one of them documentation that looks like configuration.

Same shape at `benchmark.rs:1897-1922`: dispatch *builds* `run_params` from CLI arguments and
writes them into the binding. The recipe's `params` block describes what a caller may pass;
it does not supply values.

## 5. How to fix the existing ones — benchmark first

Benchmark is the worst offender and therefore the right proof. Today it is a parallel runner
wearing a recipe's clothes: 4,000 lines of Rust in `commands/benchmark.rs`, a private store, a
private loader, a private param model, plus a `bench_round` tracker holding a second copy of
card state that the board already owns.

**The convergence, in one sentence:** an activity recipe gains a `pipeline`, the pipeline is
executed by the `PipelineExecutor` that already exists, and `affordances` select the tool
surface for citizens in that room.

```jsonc
{
  "purpose": "benchmark/swe",
  "base": "academy",
  "regions": [ /* unchanged — the board, the feed, the scoreboard */ ],

  // NEW: capabilities this room grants. Nothing else is offered here.
  "affordances": [
    { "command": "code/read" }, { "command": "code/edit" }, { "command": "code/run" },
    { "command": "work/claim" }, { "command": "work/state" },
    { "verb": "observe", "command": "cognition/observe" }
  ],

  // NEW: behaviour as data. Every step is an existing discoverable command, in the
  // step shape the executor already reads (`params`, `outputTo`, `condition`,
  // `approval`). `each` (fan-out per item) and `on` (an event edge such as
  // card.settled) are NOT in the shape yet — they are S3b in the plan.
  "pipeline": [
    { "command": "benchmark/import",  "params": { "suite": "$args.suite", "instances": "$args.instances" },
      "outputTo": "tasks" },
    { "command": "work/create",       "each": "$tasks",   // S3b
      "params": { "room": "$room.id", "title": "${item.instance_id}", "body": "${item.problem_statement}" } },
    { "command": "activity/invite",   "params": { "room": "$room.id", "members": "$args.team" } },
    { "command": "work/grade",        "on": "card.settled",   // S3b
      "params": { "card": "$event.card", "oracle": "${item.oracle}" } }
  ],

  "params": {
    "suite":  { "default": "swe-bench-verified" },
    "team":   { "default": [] },
    "driver": { "default": "citizen" }
  }
}
```

What that deletes: the private `benchmark_recipes` collection, `load_recipe`, the bespoke
param interpolation, the dispatch code that hand-builds `run_params`, and — with the board as
the single card store — most of `bench_round`. What it *keeps* is every rule doctrine already
established: benchmarks are adapters, the room is the runner, the card owns its grade.

**The migration is mechanical and testable:** each pipeline step replaces a block of dispatch,
and the acceptance is that a round dispatched from the recipe produces the same board, the
same rooms and the same verdicts as the Rust path, before the Rust path is deleted.

## 6. Everything we do is an activity — here is the ledger

Anything the system does *for* someone is an activity: a room, a pipeline, and a set of
capabilities. This is the coverage the shipped floor is supposed to prove.

| What we do | Room / regions | Pipeline does | Affordances it needs | Today |
|---|---|---|---|---|
| Chat | messages, roster | nothing | speak | ✅ recipe |
| Video call | stage, roster | mint token, join, transcribe | live/*, tts, stt | partly hardcoded |
| Project | board, messages | card lifecycle | work/*, code/*, git | ✅ recipe, inert rules |
| Benchmark round | board, feed, scoreboard | import → cards → invite → grade | code/*, work/* | ❌ 4k lines of Rust |
| Forge a model | stages, metrics | prune → quant → eval → publish alloy | foundry/*, hf/* | ❌ Rust + hand-authored alloy |
| Training / LoRA cycle | buckets, curve | gather → train → eval → gate → publish | genome/* | ❌ Rust |
| Install / onboarding | checklist | detect → fetch → verify → seat | system/* | ❌ shell scripts |
| Code review | diff, thread | fetch → review → verdict → merge | code/*, gh | ❌ agent habit, no room |
| **A user's arbitrary ask** | whatever it needs | whatever it needs | whatever it needs | ❌ impossible |

Every ❌ is the same defect: behaviour that should be a pipeline lives in Rust, so the activity
cannot be authored, cannot be varied, and cannot be shared. **The repo is an editor, not a
store of recipes** — a community's billion recipes can never live here, which makes the shipped
floor *integration coverage* and nothing more. It has to be chosen and asserted as coverage.

## 7. A random example, end to end — the one we ran by hand today

On 2026-09-10 the author ran a job search manually with a general-purpose agent: rebuilt a
résumé, verified six employers against the public record (**four claims were false**), produced
three targeted cuts, rewrote a LinkedIn profile, and pasted private career history into a
stranger's website to get a PDF. Nothing was submitted, tracked, or followed up — the parts
that decide the outcome. No room held any of it. Nothing learned.

Here is that same ask as data. Note that **no field below is job-specific**; it is a campaign.

```jsonc
{
  "purpose": "campaign/applications",
  "regions": [
    { "name": "board",    "kind": "kanban", "role": "primary",    "slot": "content" },
    { "name": "dossier",  "kind": "wall",   "role": "peripheral", "slot": "context" },
    { "name": "messages", "kind": "chat",   "role": "peripheral", "slot": "content" }
  ],
  "citizens": [ { "role": "researcher" }, { "role": "writer" }, { "role": "verifier" } ],

  "affordances": [
    { "command": "web/search" }, { "command": "web/fetch" },
    { "command": "document/render" },
    { "command": "browser/act", "approval": "human" },
    { "command": "mail/send",   "approval": "human" }
  ],

  // Real step shape: `params` / `outputTo` / `condition` / `approval` run today (S3);
  // `each` and `on` are S3b. `approval: "human"` already HOLDS a run at that step.
  "pipeline": [
    { "command": "web/search",       "params": { "q": "$args.targets" },  "outputTo": "found" },
    { "command": "work/create",      "each": "$found",                                      // S3b
      "params": { "room": "$room.id", "title": "${item.employer} · ${item.role}", "body": "${item.url}" } },

    { "command": "web/fetch",        "on": "card.claimed",                                  // S3b
      "params": { "url": "$card.url" }, "outputTo": "posting" },
    { "command": "cognition/verify", "each": "$dossier.claims", "outputTo": "sourced" },    // S3b — the step that catches the four
    { "command": "document/render",  "params": { "from": "$dossier", "for": "$posting" },
      "outputTo": "artifact" },

    { "command": "browser/act",      "params": { "url": "$card.url", "fill": "$dossier", "attach": "$artifact" },
      "approval": "human" },                       // fills the form, HOLDS at submit — a human presses send, always

    { "command": "work/state",       "params": { "card": "$card", "state": "submitted" } },
    { "command": "work/due",         "params": { "card": "$card", "in": "$args.cadence" } },
    { "command": "chat/send",        "on": "card.due",                                      // S3b
      "params": { "room": "$card.room", "text": "no reply in $args.cadence — draft a follow-up" } }
  ],

  "params": {
    "subject": { "doc": "the dossier this campaign argues for" },
    "targets": { "default": [] },
    "cadence": { "default": "7d" },
    "autonomy":{ "default": "draft", "doc": "draft | send-with-approval | send" }
  }
}
```

Read the affordance list again: **`web/search` and `web/fetch` already exist.** The three that
do not — `document/render`, `browser/act`, `mail/send` — are ordinary commands, and the eye-node
is already a real Playwright browser that observes but cannot click. The campaign is not
blocked on intelligence. It is blocked on the recipe layer being unable to say "this room may
use these verbs, and this step needs a human."

The same file with different params is a grant submission, a CFP season, a house hunt, an
insurance appeal. **That** is the infinite surface — one engine, authored data, no Rust.

## 8. The order of the fix

1. **`affordances` select the tool surface.** One filter in one place, room-scoped, falling
   back to hands. Until this lands every other recipe field is decoration. *(card c2ec4b96)*
2. **`pipeline` on the activity recipe, executed by `PipelineExecutor`.** The engine exists;
   give it the room. *(new card)*
3. **`cognition/generate-recipe` becomes a verb, with a write path to the overlay.** Then an
   ask in `#general` or a DM produces a recipe. *(cards c6648d08, dbdd6bcf)*
4. **Benchmark migrates to the authored recipe** and its private store, loader and param model
   are deleted. This is the proof, and it removes more code than it adds. *(new card)*
5. **`layout` lives or dies.** *(card d92a5941)*
6. **A ratchet:** every field the schema declares has a shipped recipe exercising it and a
   consumer reading it. Fails on an undeclared-but-unread field, the same shape as the
   eviction-decision test. *(card 92de4d78)*

## 9. The smell

If the next activity we add arrives as a new `commands/<thing>.rs` with its own store, its own
loader and its own params, we have written the fourth recipe system. The tell is always the
same and it is already in `recipe/mod.rs`: **if adding a concept requires a recompile, it was
not data.**
