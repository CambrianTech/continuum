# SWE-bench round — the no-brainer runbook

**For the next driver (human or Opus): this is the exact, verified sequence to run a
SWE-bench-Verified round through the citizens. It is different from the rust-gym path in
[`../README.md`](../README.md) (`kick.sh`), and different from gym benchmarks
(`benchmark/round --benchmark <gym>.jsonl`).** SWE-bench is NOT a gym — `benchmark/round`
will refuse it (`"not a committed gym"`). SWE runs through the kanban adapter
`benchmark/dispatch`. Every step below was walked and verified 2026-08-25.

## 0. One readiness check that saves an hour

```bash
continuum persona/roster        # need resident_count > 0
```

`benchmark/dispatch` **silently refuses** to stage cards when no citizen is resident
(*"citizens registered but NOT RESIDENT — nothing dispatched would be heard"*). If
`resident=false` for everyone, hosting has not completed — do NOT dispatch yet. See
§Troubleshooting → residency.

## 1. Run a focused round (pick your instances)

```bash
continuum benchmark/dispatch \
  --name="swe-bench-verified" \
  --instances='["astropy__astropy-13236","astropy__astropy-12907"]' \
  --assignees='["Atlas"]' \
  --drive="detached_solve" \
  --force
```

- `--name` — `swe-bench-verified` | `swe-bench-lite` | `swe-bench-pro` (see `benchmark/list`).
- `--instances` — the EXACT instance_ids, in order. Omit → the dataset head (astropy's
  C-extension build is the hard tail and leads Lite, so a `--limit N` without `--instances`
  front-loads the worst odds — name your set). Substring match, so `sympy-24152` works too.
- `--assignees` — citizen display name(s), must be resident (fails loud naming who's online).
- `--drive` — `detached_solve` (exclusive warm slot; proven; fast; **but produces no room
  turns, so it teaches nobody** — BENCHMARKS-ARE-ADAPTERS §learning) vs `citizen` (works the
  card in-room, feeds the learning flywheel; slower; needs residency solid).
- `--force` — stage even if serving isn't decode-verified yet (a fresh boot).

A fresh room is minted per run (`bench-swe-bench-verified-<epoch>`) so the round can END.

## 2. Watch it

```bash
continuum persona/roster                              # resident holds? (should stay >0)
continuum benchmark/rounds                            # in_flight + stage
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

```bash
continuum benchmark/round-report        # the round, readable by a stranger
```

## Troubleshooting (the failure modes that cost 2026-08-25)

| Symptom | Cause | Fix |
|---|---|---|
| `benchmark/round: "not a committed gym"` for `swe-bench-verified` | SWE isn't a gym; `benchmark/round` is the rust-gym/gym path | Use `benchmark/dispatch` (this doc). |
| dispatch refused: *"NOT RESIDENT"* | no citizen hosted (hosting not complete) | `persona/roster` until `resident>0`; if it never comes up, see next row. |
| `resident_count` stuck at 0 forever after a reboot | a persona `service_loop` panic aborts the loop on its first self-tick (it hosts, then dies) | `grep "service_loop aborted" ~/.continuum/logs/continuum-core-server.log` — the default panic hook's `panicked at FILE:LINE` is on **stderr** (`/tmp/continuum-core-start.log`), NOT the tracing log. Fix the panic; a dead loop = 0 residency = no dispatch. |
| core won't boot: *"airc degraded: daemon socket unreachable"* | the airc daemon the core depends on is gone/stale | ensure one live airc daemon (`airc status` starts one), remove stale `~/.airc/runtime/*.sock`, then `continuum start`. Do NOT kill the airc daemon while the core runs. |
| round shows `in_flight` but `round-stop` says none | a round left `Working` by a dead core (pre-2026-08-25 zombie) | fixed: boot reaper evicts orphaned rounds (`bench.round.orphan_reaped`). |
| `benchmark/round --benchmark <raw-swe-rows>.jsonl` → 4 empty tasks, 0 acts | raw SWE rows aren't a gym task schema; the reader made empty tasks | never point `benchmark/round` at raw SWE rows; use `benchmark/dispatch --instances`. |

## The one law

Import the task + oracle only; the ROOM is the runner; grading is the activity's outcome
score. Never build a parallel runner. See
[`../../docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md`](../../docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md).
