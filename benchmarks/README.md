# Continuum benchmarks — repeatable, honest, zero-dependency

An **extras** path. Nothing here is compiled into `continuum-core` or shipped in the product;
it is tooling for measuring ourselves against external systems. Run it, don't depend on it.

## The one hard rule

**We never depend on any opponent — Hermes, unsloth, a cloud API, anything. Ever.**

Opponents are **external, optional `/v1` endpoints you bring**. This harness imports nothing
from the product and the product imports nothing from here. An opponent is a URL — a local
`llama-server`, an unsloth gateway, ollama, a cloud API, or an **airc node** exposing an
OpenAI-compatible `/v1`. That last one is the point: it also proves *grid encapsulation* —
any model, ours or an opponent's, is just a node reachable over the same seam.

## What it measures

`coder/` — the Rust coder gym (`docs/genome/humaneval-rs.jsonl`, HumanEval-Rust): each task's
answer is **compiled and run against a hidden test** (`rustc` — real pass/fail, no self-report,
no exit-code masking). Two rows, same tasks, same grader:

| runner | what it scores | how the model is reached |
|---|---|---|
| `coder/run_ours.sh` | OUR system (RAG + tools + PX + act→observe loop) | the local model the running core serves |
| `coder/oneshot_opponent.py` | an opponent, one-shot | any external `/v1` URL you provide |

The comparison is deliberately fair-with-an-asterisk: `run_ours.sh` runs the *whole system*;
`oneshot_opponent.py` runs the opponent *one-shot* (its plain inference). The gap includes both
model-fit (we pick the best local model for the ask — the thing cloud can't do) and system lift.
To isolate the *system* lift, point `oneshot_opponent.py` at the SAME local model the core serves
and diff it against `run_ours.sh`.

## Run it

```bash
# 1. OURS — a core must be up serving your local model (cu ping → ok), model warm.
benchmarks/coder/run_ours.sh "Qwen2.5-Coder-14B (ours)" 40

# 2. AN OPPONENT — spin its /v1 up yourself (any backend), then:
python3 benchmarks/coder/oneshot_opponent.py \
    --endpoint http://127.0.0.1:8080/v1 --model hermes-3-8b \
    --label "Hermes-3-8B" --limit 40

# 3. Paste both rows into coder/SCOREBOARD.md.
```

Requirements: `rustc` on PATH (grading), `python3` (stdlib only). No pip installs, no product runtime
for the opponent side.

## Adding an opponent

There is no code to add — an opponent is a config line (endpoint + model + label). Stand it up
however you like (that is *your* dependency, never ours) and point the harness at it. See
`coder/SCOREBOARD.md` for the running results.

## The matrix (reproducible — runners × benchmarks → a chart)

`matrix.py` replaces the hand-run one-offs: one command runs every (runner × benchmark)
pair from a config and emits `CHART.md` + `results.json`.

```bash
cp benchmarks/config.example.json benchmarks/config.json   # edit runners + benchmarks
python3 benchmarks/matrix.py benchmarks/config.json
```

- A **runner** is `ours` (through the Continuum core) or `opponent` (an external `/v1` you
  bring up — llama-server, unsloth gateway, ollama, cloud, or an airc node). A future
  **team** runner (coordinated personas on one plan) drops in with no charting change.
- Put a model's **own published number** for a benchmark in its `published` map and the chart
  renders `_(claim …)_` beside what we measured identically — so an "amazing claim" meets a
  real, common benchmark reproduced the same way. Take them on their numbers.
- Add a benchmark = one config entry + a gym file. Add an opponent = one config line (stand
  its endpoint up yourself; that is your dependency, never ours).

This is the system to run across many models and many benchmarks — individuals first, then
teams that scale and learn — and chart it, repeatably.
