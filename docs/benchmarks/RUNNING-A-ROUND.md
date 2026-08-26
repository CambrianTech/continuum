# Running a Benchmark Round — the replication recipe

This is the COMPLETE path from a fresh clone to a graded, receipted benchmark
round. It exists so a run can be REPLICATED — by another operator, another
machine, or a leaderboard reviewer. Every command here is real; if a step below
ever needs a manual workaround, that workaround is a bug
(docs law: foolproof > instructions — every extra doc line is a design defect).

First proven end-to-end 2026-08-26: three SWE-bench Verified instances resolved
hands-off (astropy-12907 2/2 f2p · 13/13 p2p, astropy-14995 1/1 · 40/40,
astropy-14365 1/1 · 8/8), round driven entirely by the substrate — dispatch →
per-solve rooms → autograde → harness close → round Done — including surviving
a `continuum reboot --force` mid-solve.

## 0. What a round IS

A round is an ACTIVITY: a run room (airc, recipe-typed) holding kanban cards —
one card per benchmark instance — worked by CITIZENS (resident personas), never
by disposable solvers. The room is the runner; there is no parallel harness.
Rounds drive themselves: cards are claimed at dispatch, each solve mints (or
rejoins) its own child room, grading closes cards, each settled card fires the
next unworked one, and a Working round survives reboots — boot-resume re-parks
and re-fires automatically. **Do not hand-drive a round. Reboots are safe.**

## 1. Prerequisites (one-time per machine)

```bash
continuum start            # build + boot the headless core (or: continuum reboot)
continuum ping             # verify the version trio (build #, sha, built-at)
continuum benchmark/list   # the catalog — every runnable benchmark is one row here
```

A round needs: serving ready (the core boots the model itself) and at least one
resident citizen (the core spawns/resumes its roster on boot). Dispatch parks
and waits for both — a fresh boot needs no operator sequencing.

Datasets: SWE-class rows are fetched-and-cached on first use under
`~/.continuum/benchmarks/swe/<dataset>.rows.jsonl` (`benchmark/fetch` pulls
catalog collections explicitly). Gym-class sets ship in-repo or are generated
deterministically at build time (e.g. vision-qa).

## 2. Dispatch (kanban benchmarks: SWE-bench Verified / Lite, tool gyms)

```bash
# N instances from the dataset head:
continuum benchmark/dispatch --name=swe-bench-verified --limit=3

# Exact instances (replication: pin the list, publish the list):
continuum benchmark/dispatch --name=swe-bench-verified \
  --instances='["django__django-16938","astropy__astropy-8707"]'
```

- Omit `--assignees` to round-robin the live roster.
- Omit `--room` for a fresh run room (`bench-<benchmark>-<epoch>`).
- For a REPLICABLE sample, generate the instance list with a pinned RNG seed and
  publish the seed + list alongside the score (2026-08-26 25-round seed: 20260826,
  proportional sample over the 500-row Verified set, minus already-run instances).

Everything after the dispatch command is automatic. Walk away.

## 3. Round (gym benchmarks: eval-graded sets, e.g. hard-rs, vision-qa, DS-1000)

```bash
continuum benchmark/round --benchmark=vision-qa --persona=Atlas
continuum benchmark/round --benchmark=hard-rs          # sole online persona sits it
```

Resume is the default — a re-issued round finishes the set it started; `--fresh`
abandons and restarts (the rare exception).

## 4. Where truth lives (read these, never infer)

| What | Where |
|---|---|
| Per-instance verdict (resolved, f2p/p2p, gate_ok) | `~/.continuum/benchmarks/swe/verdicts/<instance>.json` |
| Live solve progress | `~/.continuum/progress/agent-solve-claim-<card>.json` (+ `.grade.json`) |
| Round state (cards, stage, solve rooms) | `~/.continuum/state/bench-rounds/<round>.json` (deleted on Done) |
| Round lifecycle probes | `bench.round.*`, `benchmark.dispatch`, `benchmark.autograde`, `benchmark.card.closed_by_harness` in the core log / `debug/probes/query` (result key is `events`) |
| All runs | `continuum benchmark/runs`, `continuum benchmark/rounds` |

A solve is NOT a process — `pgrep` finds nothing; solves run inside the core.
The round is DONE when `bench.round.done` fires (every card terminal).

## 5. Publishing a score (the honesty seam)

Every published number carries its REGIME, or it is not a claim:

- model + quantization tier (e.g. Ornith-1.5-35B-A3B, Q4_K_M)
- hardware (e.g. one Mac M5 Pro 64GB) and per-instance wall-clock envelope
- build SHA of the core that ran it (`continuum ping` trio)
- the instance list + sampling seed, and the verdict files as receipts
- misses attributed per the standing rhythm (her / env / harness) — env-tainted
  misses owe a retake and must not teach

Official leaderboards accept exactly these receipts: SWE-bench takes
patches + trajectories via PR (they re-grade in their own containers);
Terminal-Bench submits via tbench.ai. Contaminated sets (HumanEval and kin)
are PROHIBITED — they carry no signal.

## 6. When something looks dead

1. `continuum ping` — version trio first, always.
2. Read `served_context_window` before anything else (a mute citizen is a
   window problem until proven otherwise).
3. `debug/probes/query` for the round's probe classes — every abort path is
   probed and named; an unnamed silence is itself the bug to report.
4. Reboot freely — hesitating to reboot is the defect; rounds resume.
