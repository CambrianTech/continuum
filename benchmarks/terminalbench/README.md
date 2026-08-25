# Terminal-Bench 2.1 — the repeatable path

Everything behind the README's TB row, runnable by anyone from a fresh clone. No
operator magic: each step is one command, every output lands as a receipt, and the
denominator is *derived on your host*, never copied from ours.

## 0. Prereqs

A running core (`continuum start`), `python3` (system is fine; the opponent arm
additionally wants python ≥3.10 on PATH and bootstraps its own venv). macOS arm64
for the stock-server arm's prebuilt binary (other platforms: adjust the tarball URL).

## 1. Fetch + stage the suite

```bash
continuum benchmark/fetch --benchmark terminal-bench
```

Stages the tasks and materializes per-task oracles with the container-contract
projection (`/app`, `/tests`, and bare `python` → your real interpreter).

## 2. Gold-gate YOUR host (the honest denominator)

```bash
benchmarks/terminalbench/goldgate-sweep.sh /tmp/tb-gold.csv /tmp/tb-gg-work
```

Runs every task's OFFICIAL solution (cwd = `app/`, harness runner protected from
filename collision) then its oracle. A task only counts if the known-correct
solution passes **on your machine** — env-fails are excluded *by name* in the CSV,
never scored as model zeros. Emits `~/.continuum/benchmarks/tb21-gold-<host>.jsonl`
— your gold gym. (Expect a modest subset: many upstream tasks assume a Linux
container's apt/docker world.)

## 3. OURS — the citizen round

```bash
continuum benchmark/round --benchmark ~/.continuum/benchmarks/tb21-gold-<host>.jsonl --persona <name>
```

One verb, initiate and resume alike: interrupt it with anything (reboot, Ctrl-C, a
deploy) and the same command continues from the first ungraded task, grades kept.
`--fresh true` re-sits the set (the retake curve — label it as such). Grades stream
to `~/.continuum/progress/<persona>.jsonl`; per-task lessons stream to the persona
at grade time; solutions preserve under `~/.continuum/bench-receipts/<run>/`.

Optional arms: `--help-arm true` (peer help declared legal, exchanges receipted —
reports as OURS+help, never conflated with solo).

## 4. The opponent — their whole world

```bash
continuum stop
benchmarks/terminalbench/opponent-stock-arm.sh /tmp/tb-armB    # stock llama (pinned tag) + mini-SWE-agent
continuum start
```

Self-bootstraps the *unmodified* upstream `llama-server` at a pinned release tag
and the mini-SWE-agent venv; smoke-gates one trivial completion before burning
tasks (a harness-env failure aborts loudly — fake zeros are refused in both
directions); grades with the identical oracles. `opponent-miniswe-arm.sh <port>
<model> <outdir>` runs the same harness against *our* serving stack instead (the
cognition-isolation arm).

## 5. Record + render

```bash
continuum benchmark/record --benchmark terminal-bench-2.1 --model <id> --harness <arm> \
  --resolved <n> --total <n> --replication "<the exact command above>" --hardware <tier>
python3 benchmarks/render_results.py
```

Rows append to the committed ledger; the README section, chart, and
`benchmarks/ALL-RESULTS.md` regenerate from data. No hand-edited claims.

## Claim discipline

Two curves, always: a repeat round is the **retake curve** (the memory claim — say
so in the row note); newly gold-gated tasks are the **fresh/transfer curve**. Gold
subsets are host truths — publish your CSV beside your numbers.
