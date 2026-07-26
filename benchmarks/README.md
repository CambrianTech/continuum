# Continuum benchmarks — reproducible, honest, zero-dependency

Benchmarking is **operational**, so it lives where the operational path lives: **Rust, on the
DynCommand registry**, managed by the daemons, callable by a persona. The one exception is a
single toolchain-free script whose only job is letting an *outsider* replicate our numbers
against their own endpoint without our stack.

## Reproduce every claim — ONE command

```bash
./benchmarks/kick.sh                 # full fleet × the gym ladder, RAW + OURS + opponent-harness
./benchmarks/kick.sh --limit 10      # quick shakeout
```

This is the whole thing behind one handle (`kick.sh`): it downloads each opponent from its
authoritative Hugging Face repo into **your** cache (no operator-specific paths — see
`coder/models-fleet.json`, addressed by `gguf_repo`+`gguf_file`), serves it, runs the **same
gym + same `rustc` grader** against three arms — **RAW** (model one-shot), **OURS** (the same
model through the full Continuum cognition loop), and the competitor's own agentic harness
(**opencode**) — across a ladder of progressively harder gyms (`humaneval-rs` → `hard-rs` →
`frontier-rs`), appends every cell to the append-only ledger `RESULTS.jsonl`, and re-renders
the evidence board. Requirements are checked up front and named if missing (`rustc`, `python3`
+ `huggingface_hub`, `llama-server`; a Continuum core — `continuum start` — only for the OURS column).

**Repeatable by design:** fix a cognition bug, run it again, read the delta. **Reproducible by
design:** a stranger who cloned the repo runs the identical command and gets the identical
numbers — that is the point (leave no doubt about the claims). The sections below are the
individual pieces `kick.sh` orchestrates.

## Measure on a quiet GPU — provenance, not hope

A benchmark number is only reproducible if the GPU was doing **nothing else** when it
was taken. Share the GPU with a game, another model, a training run — or, on our box,
the live persona-serving lane while the citizens are awake and generating — and the
number still prints, just wrong, and *nothing downstream says so*. That silent
contamination is real: it once produced a `None/7` flagship cell because the sweep
time-shared the awake persona lane on `:58057`.

So the machine refuses to pretend. `coder/preflight_gpu.py` (run automatically by
`kick.sh`) queries a live core's `gpu/stats` and prints a one-line verdict —
**CLEAN** / **CONTENDED** / **UNKNOWN** — and *blocks the run* on CONTENDED unless you
accept it explicitly:

```bash
# on a Continuum box, quiet the citizens first (a humane, auto-waking nap):
continuum cognition/set-sleep-mode --persona-id <UUID> --mode sleeping --duration-minutes 90 \
    --reason "clean benchmark — auto-waking"
./benchmarks/kick.sh --gyms hard-rs --limit 8         # preflight now reports CLEAN

# on any box, if you must measure a contended GPU, the number is stamped CONTENDED:
./benchmarks/kick.sh --allow-contended
```

UNKNOWN (no core, opponent-only arms) proceeds — the opponent arms serve their own
scratch lanes, so there's nothing on our side to contend for. The rule is simple:
**a clean claim requires a clean measurement, and the machine says which one it made.**

## The one hard rule

**We never depend on any opponent — Hermes, unsloth, a cloud provider — ever.** Optional
integrations that reach what a user already runs are an asset; a forced dependency is a
weakness (and off-grid: unmanageable by our daemons).

## Run OUR models — Rust `benchmark/*` commands (on-grid, persona-callable)

The benchmark catalog + runner are Rust (`core/continuum-core/src/commands/benchmark.rs`),
mirroring the model catalog. `benchmark/run` is a thin wrapper over `cognition/eval` — one
grader, never reimplemented.

```bash
continuum benchmark/list                                              # the catalog (name, grader, tasks, runnable)
continuum benchmark/run --persona_id <UUID> --name humaneval-rs --limit 40
```

Add a respected collection (SWE-bench, LiveCodeBench, MBPP, …) = one `BenchmarkSpec` row in
`benchmark.rs`. Big datasets pull + cache like a model (follow-up: `benchmark/pull`); the
`grader` per entry says how solutions are scored (`rust` compile+run today). Later this same
registry is how a persona runs a competition by name.

## Score an OPPONENT — the one edge script

`coder/oneshot_opponent.py` — the lone Python, deliberately toolchain-free: Python stdlib +
`rustc`, imports nothing from us. Point it at any external OpenAI-compatible `/v1` (a local
llama-server, an unsloth gateway, ollama, a cloud API, or an airc node) and it scores that
model one-shot on the same gym, graded identically. This is how outsiders replicate us and how
we take a model on its own published claims — same tasks, both numbers.

```bash
python3 benchmarks/coder/oneshot_opponent.py \
    --endpoint http://127.0.0.1:8090/v1 --model hermes-3 --label "Hermes-3-8B" --limit 40
```

## Results

`coder/SCOREBOARD.md` — the running board. Ours (through the system) beside opponents (external
one-shot), same tasks, same rustc grader. Reproduce with the two commands above.
