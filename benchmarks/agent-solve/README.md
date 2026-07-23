# agent/solve battery — the whole-being agentic benchmark

Drops a persona's WHOLE self (memory ON, genome lane, tools ON — never stripped)
into seeded git workspaces via `agent/solve`, one task at a time, and grades each
task by running the repo's own assert. The matrix mode sweeps model × tier × rep
and appends aggregated rows to the canonical evidence ledger
(`benchmarks/RESULTS.jsonl`) as the `OURS` arm; opponent harnesses (hermes /
aider / opencode) join as sibling arms on the SAME tasks.

Tiers:
- **t1** — single-file bug fixes (the floor: does the act→observe loop work).
- **t2** — multi-file root-cause tracing, class invariants, base cases,
  implement-from-docstring (the instruction names the SYMPTOM, not the file).

Honesty rules (non-negotiable):
- **One mind, one task at a time.** Strictly sequential — same-persona
  concurrency degrades cognition and lane thrash confounds scores (both
  glass-boxed 2026-07-22).
- **INFRA ≠ FAIL.** Lane/pressure faults report separately; an arm with zero
  clean reps is VOID (`excluded: true`), never a fake 0%.
- **Captures on.** Every run's turn-by-turn decisions land in
  `<outdir>/captures/<persona>.jsonl` — every number glass-boxes to a mechanism.

Usage (server running; persona spawned):

    # one sequential battery + grade
    python3 benchmarks/agent-solve/bench.py runseq <persona_uuid> mylabel t1

    # the full matrix -> scoreboard.md/json + RESULTS.jsonl append
    python3 benchmarks/agent-solve/bench.py matrix <persona_uuid> /tmp/matrix-out 2

Re-render the README evidence section after a sweep:

    python3 benchmarks/render_results.py

## Tier 3 (design) — projects that RUN: websites first, then Xcode

The next rung after t2, graded like everything else — by what actually executes,
plus her own eyes:

- **t3-web tasks:** seed a minimal project (or empty repo + spec); she must make it
  RUN. Graders escalate: (a) `curl` the dev server → HTTP 200 + expected content;
  (b) a headless screenshot of the rendered page fed to her OWN vision judgment
  (the faithful-web-preview harness) and to an assert on structural markers.
  Examples: "the nav renders but the counter button does nothing — fix it"
  (seeded React/vanilla bug), "build a landing page per this spec and serve it."
- **t3-scale mechanics:** project tasks need horizons beyond one solve —
  `max_acts` rises, and multi-solve tasks decompose through the EXISTING kanban
  (work/claim) so a project is an activity containing many solves. One persona
  first; the team version (delegation across specialists) is the t4 headline.
- **t4-ios (this Mac has Xcode):** `xcodebuild` + XCTest are shell receipts like
  rustc was; simulator screenshots feed the same eyes. Grade = build succeeds +
  tests pass. Swift is where open coders are weakest — which makes it the genome
  speciation showcase: the Academy manufactures the Swift curriculum from her
  failures, and the trained adapter fills a gap no open base fills.
- **Skill acquisition is part of the tier contract:** on a domain miss, the ladder
  is genome page-in → HuggingFace adapter search (`continuum:*`) → Academy trains
  from the tier's own failure corpus. The battery IS the curriculum generator.

## Status: scaffolding, not substrate

Python here is deliberately NON-load-bearing (Joel, 2026-07-22): it only seeds git
workspaces, fires the `cu agent/solve` CLI, runs an assert, and tallies. Every
measurement-path concern — the drive loop, lane admission, patch extraction, the
persona herself — is Rust. Convergence TODO: fold these tiers into gym JSONL and grow
a Rust `agent/battery` sweep on `cognition/eval`'s existing task/grade/ledger
machinery, leaving Python only at external-harness boundaries (the Terminal-Bench
adapter is necessarily Python — that's tb's ecosystem, not ours).
