# legacy/ — quarantined, dead code. NOT in any build, test, or CI path.

This directory holds **retired** code kept only for reference while its
replacement settles. Nothing here is compiled, tested, packaged, or shipped:

- It is **not** a Cargo workspace member (see root `Cargo.toml` — members are
  explicit; `legacy/` is absent).
- It is **not** referenced by any `npm` script, `Dockerfile`, or CI workflow.
- Do **not** edit, fix, or wire anything here. If you find yourself editing a
  file in `legacy/`, stop — you're patching poison. Fix the replacement instead.

The rule (Joel): move legacy *out of the path first*, let the build/refs blow up,
and chase the breakage to the pure replacement — never work around the old thing.

## node-startup/

`parallel-start.sh` — the legacy Node start orchestrator that ran behind
`npm start`. Slow, single-threaded, and it broke on stale paths (`cd workers`
after the workers→core rename; missing `@gltf-transform/core` scene-gen) that had
nothing to do with the substrate.

**Replacement:** `tools/scripts/start-server.sh` — the pure-Rust headless start
(`cargo run` the `continuum-core-server` directly, per-platform GPU features, no
Node). Both root and `src/` `package.json` `start` now point at it. Talk to the
running core with the Rust **`continuum`** client (`continuum ping`, …) — the replacement for
the Node `./jtag`.

## benchmarks/swe/

`run_ours.py`, `grade_local.py` — the Python SWE-bench harness: clone at
`base_commit`, drive a solver, `git diff` → `predictions.jsonl`, hand off to the
official Docker grader.

**Replacement:** `benchmark/swe-solve` and `benchmark/swe-grade`, live Rust
commands on the registry since 2026-08-04, with `cognition/swe_bench.rs` owning
the protocol — dataset fetch cached on first use, clone at base_commit,
era-matched interpreter + date-pinned deps via `uv` (a Rust binary), one pytest
run, verdict resolution by node id OR bare function name. They compose
`agent/solve` in-process: no subprocess, no CLI, no polling a file we wrote
ourselves. Reproduce with `continuum benchmark/swe-solve --instance <id>`.

Quarantined here rather than deleted for the reason Joel gave (2026-08-06):
*"Once fully replaced you need to get rid of Python work by placing it in
legacy, otherwise you will keep finding it."* Deletion hides it in git history,
where it is invisible right up until someone greps and resurrects it. `legacy/`
is the one place to look, and its name is the answer.

That is not hypothetical — it is what happened here. A note in memory already
said "SWE-bench is ported, do NOT rebuild the Python; benchmarks/swe/*.py is
superseded", and the script still got driven for a whole session. Worse: a real
contamination bug found while driving it got PATCHED IN THE PYTHON, while the
live Rust path carried the identical `learn: true` defect, unfixed, because
nobody looked there. A fix applied to a dead file lands nowhere.

**Still Python, still in the active tree, NOT yet verified as replaced** (#318):
`benchmarks/coder/*.py` (matrix, harness_*, oneshot_opponent, sweep_all, …),
`benchmarks/render_results.py`, `benchmarks/{agent-solve,design-gym,project,team}/*.py`,
`tools/scripts/*.py`. `benchmark/matrix` and `benchmark/competition` are already
live and should absorb the coder set — but "already live" is not "fully
replaced", and the qualifier in Joel's rule is load-bearing. Each one moves here
only after its replacement is verified to cover it.

**The boundary, unchanged:** Python as OUR infrastructure is banned; Python as
the SUBJECT under test (flask's and sympy's pytest suites, a persona's own
generated code) is the benchmark itself. We write none of the former.
