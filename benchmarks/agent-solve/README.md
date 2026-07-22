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
