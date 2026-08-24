# Gold-Gate Every Gym — no citizen round until the harness proves it can grade

**Born 2026-08-23**, the night the MirrorCode workspace-capture bug surfaced:
the eval world clones the continuum repo, the repo's root `Cargo.toml`
declares a workspace, and every staged crate without a `[workspace]` table was
refused by cargo — so the grader failed every Rust mirror task
deterministically, the citizen's own self-test builds failed with the same
cryptic error (producing the "endless analysis" behavior we nearly attributed
to cognition), and **no MirrorCode Rust build had ever actually run** across
every round ever recorded. Three weeks of numbers, void. Joel caught it by
refusing the cognition story twice: *"I'm skeptical it's not tool issues."*

The SWE plan already carried the principle for era-envs (2b: "gold-gate every
env class — an env is only accepted if the GOLD patch passes in it; that is
the positive control that makes a 0 mean 'model failed' instead of 'env
lied'"). This doc generalizes it to LAW:

**No gym runs a citizen round until `benchmark/verify` has executed that
suite's harness end-to-end in a throwaway eval world and proven it can reach
GRADING — distinguishing env-fail from honest-fail by construction.**

## The seam (design, not yet built)

Per-suite, on the existing adapter (BENCHMARKS-ARE-ADAPTERS: this is adapter
data, not a parallel runner):

- `gold_stub` — the minimal answer that must reach grading: for MirrorCode a
  compilable `fn main() {}` at `solution_file` (expected outcome: case
  results with 0 passes — NOT a build/env error); where a true gold answer is
  cheap (recorded reference behavior, a known patch), use it and expect PASS.
- `env_error_signatures` — the suite's known env-failure shapes (workspace
  capture, `command not found`, harness tracebacks) so the gate's verdict
  names the defect, not just "failed".

`benchmark/verify` gains the gate pass: provision throwaway world → run task
1's `setup_shell` → write `gold_stub` → run `dod_shell` → classify. A suite
with no stub declared is reported `ungated` (loud), never silently skipped.

## Why not built the night it was designed

The outlier-validation rule (CLAUDE.md): the seam should be designed against
the two most DIFFERENT suites — MirrorCode (cargo build + case harness) and
Terminal-Bench (shell-native tasks, no compiler) — and Terminal-Bench's first
round is next session. Building the abstraction against one suite tonight
risks exactly the wrong-interface rework the rule exists to prevent. The
MirrorCode question ("does it grade honestly now?") is being answered
empirically by the first post-fix round in flight.

## Sequence

1. First honest MirrorCode round (running, 2026-08-23 evening).
2. Build the gate against MirrorCode + Terminal-Bench as the outlier pair.
3. Terminal-Bench 53 and swe-rebench rounds only AFTER their gates pass.
4. Every future suite: gate first, rounds second — the fetch command's readme
   says so next to the fetch example.
