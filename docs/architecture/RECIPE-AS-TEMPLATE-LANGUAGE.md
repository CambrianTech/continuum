# A recipe is a template language: the activity's machine, as data

**Status:** design, 2026-10-05. Owner: Fable. Peers: BigMama (the review stage, card fa4aaaaa), Cormac, Kimi (an author of recipes, not only a subject of them).

**Joel, 2026-10-05:** *"It's like we're building a YAML like we did in CloudFormation back in my AWS days, and a lot of other configs. You can plug in the workflow. We did this at Cambrian in our pipeline design for computers and queues. This is kind of what a recipe is. We can develop a good extensible template into a good schema so that recipes really provide a programming definition of the machine itself: its constituent parts and operations. And recipes are not ALL shipped with the repo: other than the ones we need for Continuum itself, a human or persona ought to be able to use it for any purpose, like any good template language."*

This document unifies four that circle it: `RECIPE-EXECUTION-RUNTIME.md` (recipes are data, the executor walks them), `HOW-AN-ACTIVITY-DECLARES-ITS-JUDGE.md` (the judge is declared in params), `ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md` (the round's states belong to its recipe), and `RECIPES-ARE-THREE-SYSTEMS-THAT-NEVER-MEET.md` (the audit). It names what the schema already is, the two things it lacks, and how recipes leave the repo.

## 1. The machine, in CloudFormation's and GitHub's words

| CloudFormation | GitHub Actions | A recipe (`experience::recipe::ExperienceRecipe`) | Status |
|---|---|---|---|
| `Parameters` | `inputs` | `params` (declared, typed by default, bound at `activity/spawn --params`; `repo` since #4762) | exists |
| `Resources` | the runner's surface | `regions` (board, messages, roster, wall…), `layout` | exists |
| `Outputs` | artifacts | the run receipt `(recipe_id, version, content_hash)`; the card ledger | exists |
| IAM / `permissions` | `permissions`, `environment` reviewers | `citizens[].role` + `affordances` (verb → command, with a proof) | exists; roles declared, not yet enforced per member |
| `jobs.<id>.steps` | `steps` | `pipeline` (a list of commands with params) | exists, linear only |
| — | **`on:`** (triggers), `if:` (conditions), `needs:` | **missing**: stages triggered by typed room events, with a condition and a role | **new** |
| a template in S3 | a workflow in any repo | **authoring and sharing** by humans and personas, outside the binary | **new** |
| `!Ref`, nested stacks | reusable workflows | `base` (inheritance) | exists |

So the schema is two fields short of the machine, and the rest is already typed Rust exported through ts-rs. Nothing here is a new system; it is the same struct with `on:` and an install path.

## 2. The two missing fields

### 2.1 `stages`: the workflow, event-triggered

```jsonc
"stages": [
  {
    "on": "work_submitted",            // a typed room event (airc_work::WorkEvent kinds, chat kinds, schedule)
    "when": { "card.state": "review" },// a condition over the event and the room's state; absent = always
    "by":   ["reviewer", "owner"],     // roles allowed to run it; absent = any member; "self" allowed by policy
    "steps": [ { "command": "work/review", "params": { "..." : "..." } } ],
    "until": { "reviews.passed": 1 }   // the stage's completion condition; the first use is the review policy
  }
]
```

- `on` is a typed event kind, never a string the executor pattern-matches: the same `RoomWork` decode the perception feed and the curriculum use (#4731, #4765). One decode, N consumers; the executor is one more.
- `by` is the permission: roles from `citizens[].role`, resolved against the room's membership (a membership fact, the follow-up named on fa4aaaaa). `self` is a policy word, not a role.
- `until` is how a stage says what "done" means, so the gate executes whatever the recipe declares instead of a flag in code. The review policy (`{required, roles, self}`) is the first `until`; merge, a gene's publication and a consent are the next, with no new gate code.
- `pipeline` stays as the degenerate case: a stage with `on: "spawn"` and no `until`.

### 2.3 Steps name their components by URI; the scheme picks the adapter

**Joel, 2026-10-05:** *"I love designs like that because often you can insert shell or code if necessary, and we allow it. If some adapter took it. Maybe there's a URI to the components they're using, like the commands already have."*

A step's `command` today is a command name. It becomes a component URI, and the scheme chooses the adapter that runs it (the OpenCV-style registry CLAUDE.md asks for: one interface, N implementations, selected at runtime by name):

| Scheme | Runs | Notes |
|---|---|---|
| `command:` (default, bare `work/review` still means this) | the command system | the extension surface stays the command system itself |
| `shell:` | `code/shell` with the step's script | allowed, recorded as `shell` in the run receipt, and her hands policy applies exactly as it would to `code/shell` from her turn; CloudFormation's custom resource, GitHub's `run:` |
| `recipe:<id>` | another recipe, as a nested activity | reusable workflows; `base` is inheritance, this is composition |
| `gene:<id>`, `model:<id>` | later: page a gene in for the stage, or run a step on a named model | the same resolver genes use |

The URI is validated at install against the adapters this core has (an unknown scheme is refused by name), and the run receipt records which adapter ran each step, so a recipe that reaches for `shell:` says so in its provenance rather than hiding it inside a command.

### 2.2 The round's lifecycle is a set of stages

`ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md` wanted the round's states in the recipe. With `stages` they are: `on: "card_done"` → `until: {reviews.passed: N}`; `on: "all_cards_settled"` → `steps: [benchmark/close]`. `bench_round::review_gate` and the sibling-card code are then the benchmark recipe's stages, and the general executor runs them like any room's.

## 3. Recipes leave the repo

Only the recipes Continuum needs for itself ship in the binary (`chat`, `project`, `profile`, `benchmark`…). Everything else is authored and installed as data:

- **Authoring.** A file on disk (`activity/recipes install <path>`), or a persona's act: she writes the JSON and installs it, like any other hand (`citizens-have-a-social-life-dm-cowork-hobbies-invite-and-author-activities`). Validation is the schema: an unknown field, an undeclared param, a stage on an unknown event kind, or a `by` role the recipe never declared is refused at install with the schema's own words.
- **Identity and versions.** `id` is stable, `version` bumps on install, the content hash rides the run receipt (already so). A recipe that `base`s another carries that lineage.
- **Sharing.** The same three-source resolver genes get (`GENE-REUSE-FORK-MINT.md` §5): her store → the mesh → the HF repository, with the recipe's signature and lineage verified. A recipe is a small text artifact with a card; it rides the genome repository's shape, not a second registry.
- **The schema is the contract.** `ExperienceRecipe` exports through ts-rs (as everything on the wire does) and as a JSON Schema at `protocol/schema/recipe.json`, versioned with the struct, so an author outside this repo (a human with an editor, a persona with `code/write`) has the same definition the executor validates against.

## 4. What this subsumes (deleted when it lands)

- `bench_round::review_gate` / `review_required(card)` keyed on a round (card fa4aaaaa does the first half: the policy read from the room).
- `open_review_card`'s refusal of non-benchmark titles and its SWE-shaped body: the body is the benchmark recipe's stage params.
- Any future "flag on the round" that is really a stage.

## 5. Falsifiers

- a workflow decision found in Rust that the schema could have declared (a `review_gate`, a `merge_gate`, a `needs_consent` bool);
- a recipe a persona authored that the executor cannot run but the schema accepted (schema and executor disagree);
- a recipe shipped in the binary that Continuum itself does not need.

## 6. Build order

1. `stages` on the struct + schema export + install-time validation (refusals in the schema's words). Receipt: `project.json` declares its review policy as a stage; a malformed recipe is refused by name.
2. The executor consumes `on` from the inbound seam (the one decode) and runs `steps` under `by`; `until` evaluated on the room's state. Receipt: the review policy (fa4aaaaa) runs as a stage; `review_gate` deleted.
3. `activity/recipes install <path>` for humans; the same verb in a persona's hands. Receipt: Kimi authors a recipe for an activity of her own (a hobby room, her own rules) and spawns it.
4. Sharing through the resolver (store → mesh → HF). Receipt: a recipe authored on one node spawns on another by id.

Each lands on canary green with a peer word; the citizens are asked.
