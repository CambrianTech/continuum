# SWE-bench round — the no-brainer runbook

**For the next driver (human or Opus): the exact sequence to run a SWE-bench-Verified round
through the citizens.** A benchmark is an ADAPTER, not a runner: `benchmark/import` turns the
suite's tasks into card rows, the activity's recipe posts them into its own room with
`work/create`, and the citizens in that room work the cards with their own hands. Nothing
dispatches, claims, solves or grades on their behalf — the `benchmark/dispatch`, `benchmark/round`,
`benchmark/rounds`, `benchmark/round-report` verbs and the detached `agent/solve` are deleted
(card c4f4b8cd step 2).

## 0. One readiness check that saves an hour

```bash
continuum presence/directory    # need a peer with resident: true
```

A card nobody resident can hear is dead on arrival. If `resident=false` for everyone, hosting
has not completed — do NOT spawn the round yet. See §Troubleshooting → residency.

## 1. Run a focused round (pick your instances)

```bash
continuum benchmark/import --suite="swe-bench-verified" \
  --instances='["astropy__astropy-13236","astropy__astropy-12907"]'      # preview: the card rows, no side effects

continuum activity/spawn --name="swe-focused" --recipe="benchmark/round" \
  --params='{"suite":"swe-bench-verified","repo":"astropy/astropy","instances":["astropy__astropy-13236","astropy__astropy-12907"],"team":["Atlas"]}'
```

- `suite` — `swe-bench-verified` | `swe-bench-lite` | `swe-bench-pro` (see `benchmark/list`).
- `instances` — the EXACT instance_ids. Omit → the seeded sample (`sample` + `seed`) or the suite
  head (astropy's C-extension build is the hard tail and leads Lite, so a bare `limit` front-loads
  the worst odds — name your set). Substring match, so `sympy-24152` works too.
- `team` — citizen display name(s) invited into the run room; must be resident. Empty seats
  nobody: nothing seats a citizen into a round she did not join.
- A fresh room is minted per run, so the round can END.

## 2. Watch it

```bash
continuum presence/directory                          # resident holds? (resident: true stays)
continuum work/list                                   # the cards on the run room's board
continuum debug/probes/query --class=persona.act.observed --since-ms=300000   # is she programming?
ls -lt ~/.continuum/benchmarks/swe/verdicts/*.json    # grades land here (mtime = graded-now)
```

The verdict files are a **shared cache keyed by instance** — a file's *content* is the last
grading of that instance across ALL runs; its *mtime* tells you if THIS round graded it.
Filter by mtime, never trust the file list as the round's score.

Read one grade honestly:
```bash
python3 -c "import json; d=json.load(open('$HOME/.continuum/benchmarks/swe/verdicts/astropy__astropy-13236.json')); \
print('resolved',d['resolved'],'| F2P',d['f2p_passed'],'/',d['f2p_total'],'| P2P',d['p2p_passed'],'/',d['p2p_total'],'| gate_ok',d['gate_ok'])"
```
`gate_ok=true` + `F2P 0/N` = an HONEST miss (env certified, model under-fixed). `gate_ok`
false or an `error` = infra — never scored as a model 0.

## 3. Report

The round's state is its room: read the board and the chat there, the way a citizen does. The
verdict files above are the receipts a stranger can check.

## Troubleshooting (the failure modes that cost 2026-08-25)

| Symptom | Cause | Fix |
|---|---|---|
| cards posted, nobody answers | no citizen resident (hosting not complete) | `presence/directory` until a peer reads `resident: true`; if it never comes up, see next row. |
| `resident_count` stuck at 0 forever after a reboot | a persona `service_loop` panic aborts the loop on its first self-tick (it hosts, then dies) | `grep "service_loop aborted" ~/.continuum/logs/continuum-core-server.log` — the default panic hook's `panicked at FILE:LINE` is on **stderr** (`/tmp/continuum-core-start.log`), NOT the tracing log. Fix the panic; a dead loop = 0 residency = no dispatch. |
| core won't boot: *"airc degraded: daemon socket unreachable"* | the airc daemon the core depends on is gone/stale | ensure one live airc daemon (`airc status` starts one), remove stale `~/.airc/runtime/*.sock`, then `continuum start`. Do NOT kill the airc daemon while the core runs. |

## The one law

Import the task + oracle only; the ROOM is the runner; grading is the activity's outcome
score. Never build a parallel runner. See
[`../../docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md`](../../docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md).
