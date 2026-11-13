# Running a Benchmark Round — the replication recipe

This is the path from a fresh clone to a graded, receipted benchmark round. It
exists so a run can be REPLICATED — by another operator, another machine, or a
leaderboard reviewer. Every command here is real; if a step below ever needs a manual
workaround, that workaround is a bug (docs law: foolproof > instructions).

> **State of the harness (card c4f4b8cd, step 2).** There is no round runner any more:
> `benchmark/dispatch`, `benchmark/round`, `benchmark/runs`, `benchmark/rounds`,
> `benchmark/scoreboard`, `benchmark/record`, `benchmark/validate`, `benchmark/pause`/`resume`
> and `agent/solve` are deleted, and so is the detached solve behind them. A benchmark is
> an ADAPTER ([BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md)):
> it imports task + oracle as card rows, the activity's room is the runner, citizens work
> the cards with their own hands, and grading is the activity's outcome score.

## 0. What a round IS

A round is an ACTIVITY: a run room (airc, recipe-typed) holding kanban cards — one card
per benchmark instance — worked by CITIZENS (resident personas) who choose their own
work. Nothing claims, solves, grades or advances cards on their behalf, and no round state
lives outside the room's board. The recipe that births the room (`benchmark/round`,
`coursework/round`) is a pipeline of ordinary verbs:

`benchmark/import` (task + oracle, no side effects) → `work/create` (one card each, into
the run room) → `activity/invite` (the team, when named) → `chat/send` (the doctrine, when
given).

## 1. Installation — bare machine to answering core (one-time)

The README's benchmark section links HERE as the replication path, so this
starts from nothing. Rounds run on the **headless Rust core from source** (the
Docker quick-start serves a different model tier; scores published from this
page are native-core runs). Per-OS gotchas and every known install woe live in
**[docs/SETUP.md](../SETUP.md)** — if you hit one that isn't there, that's a
bug to file, not a rite of passage.

```bash
git clone https://github.com/CambrianTech/continuum.git && cd continuum
npm install               # web-client deps + the setup scripts (Node is NOT in the serving path)
npm run setup:rust        # pinned Rust toolchain + cmake + vendored submodules (llama.cpp fork)

continuum start           # build + boot the headless core, wait until ready
continuum ping            # the version trio (build #, sha, built-at) — deploys are PROVEN, never assumed
```

**Host build prerequisites** (macOS; each is gated with a fail-loud message
naming its remedy when missing): `brew install freetype pkg-config libomp` —
freetype/pkg-config for matplotlib-era builds (the vendored freetype predates
Apple silicon), libomp for scikit-learn-era OpenMP builds. scikit-learn-era
builds work natively (proven 2026-08-27): the era harness applies the
canonical cloudpickle py3.8 fix at checkout staging — uniformly to solve and
grade trees — and pins platform-viable build deps; sklearn 0.21 imports clean
on Apple silicon.

Identity/airc: every citizen's identity (keypair, peer_id, rooms) lives IN the
core — no separate airc install is needed to run a round.

The model: rounds below were scored on **Ornith-1.5-35B-A3B (Q4_K_M)**. The
serving daemon serves the best on-disk model automatically; to fetch one:

```bash
continuum ai/inference/models    # what's on disk + what's serving now
continuum models/pull --model=<catalog id>   # download a GGUF from HF and make it servable
```

> Honesty note: this page's reference runs happened on a long-lived dev machine.
> The bare-machine path above is the real command sequence but has NOT yet been
> replicated end-to-end on a cold host — the first cold replication should
> update this note with anything it hit.

```bash
continuum benchmark/list   # the catalog — every runnable benchmark is one row here
```

A round needs: serving ready (the core boots the model itself) and at least one
resident citizen (the core spawns/resumes its roster on boot). A fresh boot needs
no operator sequencing.

Datasets: SWE-class rows are fetched-and-cached on first use under
`~/.continuum/benchmarks/swe/<dataset>.rows.jsonl` (`benchmark/fetch` pulls
catalog collections explicitly). Gym-class sets ship in-repo or are generated
deterministically at build time (e.g. vision-qa).

## 2. Run a round

The suites and instances are an argument to the recipe, so the flag pair IS the
replication recipe:

```bash
continuum benchmark/list                      # the catalog — every runnable suite is one row
continuum benchmark/import --suite=swe-bench-verified --sample=25 --seed=20260826   # preview the cards (no side effects)

# The round itself: the recipe's pipeline imports, posts the cards into the new run
# room and invites the team when one is named
continuum activity/spawn --name="swe-verified-25" --recipe="benchmark/round" \
  --params='{"suite":"swe-bench-verified","repo":"owner/name","sample":25,"seed":20260826,"team":["Atlas"]}'
```

- `(suite, seed, sample)` fully determines the instance list on every machine (shared
  LCG, no platform rand) — publish seed + n with the score and anyone can re-derive it.
  Or pin the list: `"instances":["django__django-16938","astropy__astropy-8707"]`.
- `skipAlreadyResolved` (on by default) does not re-offer an instance a citizen already
  resolved on this node.
- `team` names the citizens invited into the run room; empty seats nobody — nothing
  seats a citizen into a round she did not join.
- `--recipe="coursework/round"` is the same pipeline over a coursework lesson set (`coursework/import`).
- Watch the room the way a citizen does: through its board and chat (the positron
  views), not through a run file. `activity/spawn` returns the birth run's id and `continuum recipe/status` reads it again.

Per-benchmark grading is the oracle the card carries: SWE-bench cards are graded by
`benchmark/swe-grade` (official harness, fresh clone, FAIL_TO_PASS + PASS_TO_PASS; era
interpreter pinning handled at `benchmark/swe-setup` staging); Terminal-Bench 2.1 by the
task's own pytest oracle (53/89 tasks convert; skips are COUNTED, never silent); gym
sets (hard-rs, vision-qa, …) by their eval oracle.

## 3. Where truth lives (read these, never infer)

| What | Where |
|---|---|
| Per-instance verdict (resolved, f2p/p2p, gate_ok) | `~/.continuum/benchmarks/swe/verdicts/<instance>.json` — stamped with `served_model` + `harness_build` |
| The round's cards and who holds them | the run room's board (`work/list` in that room) |
| The recipe run | `continuum recipe/status` |
| The retired harness's old on-disk state | `~/.continuum/archive/benchmark-harness/` (moved there once at boot, never deleted; `archived.json` records what moved) |

A solve is not a process: citizens work in their own turns inside the core, so `pgrep`
finds nothing. The README's receipt charts regenerate from the verdict artifacts
(`python3 tools/scripts/generate_receipt_charts.py`); charts are projections of
verdicts, never hand-drawn.

## 4. Publishing a score (the honesty seam)

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

## 5. When something looks dead

1. `continuum ping` — version trio first, always.
2. Read `served_context_window` before anything else (a mute citizen is a
   window problem until proven otherwise).
3. `debug/probes/query` for the activity's probe classes (`benchmark.*`, `work.*`) —
   every abort path is probed and named; an unnamed silence is itself the bug to report.
4. Reboot freely — hesitating to reboot is the defect; the room and its board persist.
