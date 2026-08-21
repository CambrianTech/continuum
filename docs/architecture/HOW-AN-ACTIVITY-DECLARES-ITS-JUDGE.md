# How an activity declares its judge

**Status:** design settled, not built. Written 2026-08-21 after a live audit found the
adapter interface present and unconsumed.

Companion to [ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md](ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md),
which specifies the round's STATES (`STAGING → READY → WORKING → GRADING → DONE` +
scorecard). That doc treats grading as a *state*. This one answers the question it
leaves open: **who does the judging, and how does the activity say so.**

Under [BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md](BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md):
integrations adapt INTO our rooms and activities, never the reverse.

## The constraint (Joel, 2026-08-21)

> "Grading isn't part of all recipes either. It's just benchmarks recipe." … "a lot of
> activities, especially since we're learning, will have formulaic grading for score.
> It's just a lot of different ways here and some rooms will be more open ended, take
> general chat for example."

Two things follow, and they kill two tempting designs:

1. **`grader` does NOT go on `ExperienceRecipe`.** Putting it on the base type stamps an
   exam concept onto every chat room, profile page and drawing class forever. Grading is
   an *eccentricity of activities that score*, not a substrate concern.
2. **Not every judge is code.** If a drawing exercise needs a Rust adapter to be scored,
   we have forced code where a formula belongs — which is "adapt to us" violated in
   reverse. Most learning activities want a formula over facts the room already observes.

Personality and role stay unlimited: teacher / QA / student are **genome + recipe
selection**, never hardcoded relationships. Only the adapters in/out and the judging are
structural. See [[model-the-literal-institution-not-the-ml-abstraction]].

## Three tiers

| Tier | Who judges | Where it's declared | Example |
|---|---|---|---|
| **None** | nobody | recipe declares nothing | `chat.json` — open-ended; a forced score would be a lie |
| **Formulaic** | a scoring expression over observed facts | **recipe DATA** | did the artifact appear, do the tests pass, how many acts, did she ask for help |
| **Adapter** | `BenchmarkAdapter::grade` | recipe names the adapter | SWE-bench (apply patch, run the repo's tests), a robot trajectory replay |

Formulaic is expected to be the COMMON case for learning activities. Adapter is for
judgment that genuinely requires execution.

## Where it lives: `params`, not the base type

`ExperienceRecipe` already carries per-activity knobs as `params` — typed, defaulted,
authored as JSON, invisible to recipes that don't declare them (#433). `benchmark.json`
already uses this for `suite` (documented as *"which task manifest to import — task and
oracle only"*, i.e. the adapter-IN), plus `instances`, `team`, `budget`.

The judge is another param of the same kind. **Zero schema change to the base type**, and
`chat.json` / `profile.json` / `video-chat.json` never grow a field they'd have to ignore.

## The finding that motivated this

Audited live, 2026-08-21:

- `cognition::benchmark::BenchmarkAdapter` **exists and is the right shape** — `dataset()`
  in, `tasks() -> Vec<EvalTask>` normalizing into our canonical form, `grade(task,
  outcome) -> BenchGrade` out. `TaskOutcome` is backend-neutral (spoken / patch /
  workspace / harness_passed). Its own doc: *"the ONLY per-benchmark parsing; everything
  downstream is generic."*
- Registration is `inventory` self-registration keyed by name, resolved via `get(name)` —
  dynamic discovery, no central list.
- **It has no production consumer.** Every reference tree-wide is the definition site, the
  one adapter (`humaneval-rs`) registering itself, or that adapter's own test.
- The module header names `benchmark/dispatch` as the runner that adapts tasks in.
  `commands/benchmark.rs` does not reference the adapter at all.
- SWE grading goes through hardcoded `swe_bench::grade` / `grade_swe`, bypassing the
  interface entirely.

So the interface is built and unwired, and the benchmark we actually care about routes
around it. No evidence this was a deliberate removal (unlike the `NO birth stamp` case in
`airc_runtime.rs`, which documents its own deletion) — but that is an inference from the
absence of a marker, so treat it as likely rather than certain until someone confirms.

## Build order

1. **Formulaic first.** It is the common case, it is pure data, and it needs no adapter.
   A scoring expression over facts the activity already records; per #371 gates multiply
   and objectives weigh.
2. **Then route the adapter tier through the registry** — dispatch resolves the named
   adapter and calls `grade`, deleting the hardcoded SWE branch rather than adding a field.
3. **`TaskOutcome` grows when a real second modality forces it.** It carries mouth + hands
   + workspace today, which covers code and answers. A drawing or a robot trajectory will
   want more. Grow it against a real activity, never speculatively.

## Acceptance

The same sentence must describe an exam, a life-drawing critique, and a robot that either
picked up the block or didn't: *the activity declares its judge; the substrate runs it.*
If adding a new kind of activity requires editing the base recipe type or a `match` in the
benchmark subsystem, this is not done.
