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

## ONE kind: adapters, list-valued, empty allowed

**Corrected 2026-08-21 (Joel).** An earlier draft of this doc proposed three *tiers* —
none / formulaic / adapter. That was a false trichotomy and it would have cost a DSL:

> "Or just always adapter(s) with none as an option and maybe an ability to chain or
> interlink them. So formulaic or not, anything can plug in. Just build the ones you
> initially need is my motto."

There is ONE concept:

- **A judge is an adapter.** `BenchmarkAdapter` — already the interface.
- **"None" is the empty list.** `chat.json` names no adapters; nothing scores it, and no
  code path special-cases "the ungraded kind".
- **"Formulaic" is an adapter that evaluates a formula.** Not a tier, not a schema
  variant — one implementation among many, written once and then named by any recipe
  that wants it, parameterised like anything else.
- **Several adapters compose.** This is where #371's "gates multiply, objectives weigh"
  lives: a gate adapter and an objective adapter, both named, their results combined —
  rather than a scoring grammar invented up front to express the same thing inside one
  field.

The trichotomy would have forced a scoring-expression language into the substrate with
no live consumer. One interface, N implementations, is the polymorphism rule this
codebase already runs on (CLAUDE.md § POLYMORPHISM PATTERN): one interface, many
implementations, runtime selection by name, no recompilation.

**Chaining / interlinking is deliberately left open.** Sequential composition is
obvious; whether adapters can feed each other (one's output as another's input), and how
gate-vs-objective results combine, is not settled here and should be settled by the
first activity that actually needs it — not speculated now.

## Where it lives: `params`, not the base type

`ExperienceRecipe` already carries per-activity knobs as `params` — typed, defaulted,
authored as JSON, invisible to recipes that don't declare them (#433). `benchmark.json`
already uses this for `suite` (documented as *"which task manifest to import — task and
oracle only"*, i.e. the adapter-IN), plus `instances`, `team`, `budget`.

The judge(s) are another param of the same kind — a LIST of adapter names (with their own
params), defaulting to empty. **Zero schema change to the base type**, and `chat.json` /
`profile.json` / `video-chat.json` never grow a field they'd have to ignore: they simply
don't declare it, and "not declared" already means "no judge" without any special case.

Empty-by-default is what makes "none" free rather than a third code path.

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

## Build order — "just build the ones you initially need"

1. **Route what already exists through the registry.** `benchmark/dispatch` resolves the
   adapter(s) the recipe names and calls `grade`, DELETING the hardcoded `swe_bench::grade`
   branch rather than adding a field beside it. This is a subtraction, and after it the
   one live benchmark runs on the same path any future activity will.
2. **Write a second adapter only when a second activity exists.** Not a formula DSL, not a
   speculative "generic scorer" — the actual judge the actual next activity needs. The
   registry is `inventory`-based, so a new adapter is a file that self-registers; there is
   no central list to grow and no reason to pre-build.
3. **Composition when two adapters are genuinely named together.** Chaining is cheap to
   add once a real recipe wants a gate AND an objective. Designing the combination rule
   before that is guessing.
4. **`TaskOutcome` grows when a real second modality forces it.** Mouth + hands +
   workspace covers code and answers today. A drawing or a robot trajectory will want
   more. Grow it against a real activity, never speculatively.

The through-line: every step is triggered by something real needing it. The failure mode
this ordering exists to prevent is building the general mechanism first and discovering
its shape was wrong when the second case finally arrives.

## Acceptance

The same sentence must describe an exam, a life-drawing critique, a robot that either
picked up the block or didn't, and a chat room nobody scores:
*the activity declares its judge(s); the substrate runs them; declaring none is normal.*

If adding a new kind of activity requires editing the base recipe type, adding a `match`
in the benchmark subsystem, or introducing a second KIND of judge alongside adapters,
this is not done.
