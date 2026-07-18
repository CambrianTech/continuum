# SWE Project Sourcing — feeding the benchmark boards AND the training flywheel

**Why (Joel, 2026-07-10):** "finding a lot more good SWE projects. We are lean
on benchmarking and training right now." Same corpus serves both: tasks that
PROVE the loop (boards vs opencode/Hermes-CLI/aider) and tasks that TRAIN it
(teacher→student curricula, mistake-driven pairs). One sourcing effort, two
projections — the recipe/gym duality `genome/job-create` already encodes
(dataset + evalSet are "two projections of the same recipe").

## Selection criteria (what makes a repo gym-mineable)

1. **Strong test suite** — the grader is the repo's own tests; no hand-written
   oracles. `cargo test` green on checkout is the entry bar.
2. **Small-to-medium** — fits a persona workspace + context window; whole-crate
   comprehension possible for a 24B (≲15k LoC ideal).
3. **Permissive license** (MIT/Apache-2.0) — outputs feed published LoRA
   layers ([[win-every-model-out-of-box-via-lora]]).
4. **Real-world shape** — modules, error handling, API design; not toy
   single-file katas (humaneval-rs already covers function-level).
5. **Auto-generatable tasks** — the mine is MUTATION-BASED: break something
   (revert a bugfix commit, mutate a function, delete a branch), the repo's
   tests fail, the persona repairs to green. `genome/teach` already implements
   write→error→fix→pass; this extends it from snippets to real projects.

## Tier 1 — Rust crates (the native gym; audit each against criteria first)

Small, famously well-tested, permissive:
- `bitflags`, `semver`, `humantime`, `arrayvec`, `smallvec`, `tinyvec`
- `memchr`, `itoa`, `ryu` (perf-critical, property-tested)
- `csv`, `toml`, `serde_json` (parsers — rich error paths, table-driven tests)
- `pulldown-cmark` (spec-driven test corpus), `regex-lite`
- `once_cell`, `thiserror`, `anyhow` (API-design tasks)
- `uuid`, `time`/`chrono` (edge-case dense)

Mining modes per repo:
- **Bugfix-revert tasks**: walk git history for commits touching one function
  with a test added in the same commit → revert the fix, keep the test → task.
  (Auto-verifiable, difficulty-gradeable by diff size; this is the SWE-bench
  construction, done locally on Rust.)
- **Mutation tasks**: `cargo-mutants` generates surviving/killed mutants →
  killed mutants ARE tasks (test suite localizes the fault).
- **Feature-stub tasks**: delete a small public fn body → `todo!()` → tests
  fail → reimplement from docs + tests.

## Tier 2 — Established boards (comparability with the field)

- **SWE-bench Lite / Verified** (Python): THE headline lever, needs Docker;
  runner + workspace-root seam already designed ([[swe-bench-runner-and-workspace-root-seam]]).
- **SWE-bench Multilingual / SWE-PolyBench** (Java/JS/TS/Go): same harness
  shape, broader claim.
- **Aider's polyglot benchmark** (Exercism 225 tasks, 6 languages): directly
  comparable to a rival's own published board — beating aider on aider's
  benchmark is a headline.
- **RustEvo / CRUXEval-style**: function-level; lower value than project-level
  but cheap comparability.

## Tier 3 — Our own projects as living curricula

The kanban already carries them (Conway, Snake, wordstats): milestone-
structured project cards, pair-split between personas — the coordination
signal doubles as training data ([[coordination-learning-flywheel]],
[[project-scale-work-games-and-apps]]). Every repaired task from Tier 1
mining lands here as a card a persona can CLAIM — the same field, so the
emergent-society dynamics (peer help, review, division of labor) generate
the multi-party curricula no static benchmark contains.

## Build order

1. **`gym/mine` command** (outlier A: bugfix-revert on ONE crate, e.g.
   `bitflags`): clone → walk history → emit task JSONL in the existing
   EvalTask schema (`solution_file`/`dod_shell` graded). Reuses gym_grader.
2. Run the mined set through `cognition/eval` (OURS) + `oneshot_opponent`
   (RAW) + the 3 CLI harnesses → new board row: PROJECT-level, where the
   loop's recovery + dialect-native hands actually differentiate.
3. Feed failures through the mistake-driven loop (#122) → curricula →
   `genome/job-create` (q4 path now works locally).
4. SWE-bench Lite behind Docker as the external headline.
