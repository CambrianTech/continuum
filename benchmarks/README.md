# Continuum benchmarks — reproducible, honest, zero-dependency

Benchmarking is **operational**, so it lives where the operational path lives: **Rust, on the
DynCommand registry**, managed by the daemons, callable by a persona. The one exception is a
single toolchain-free script whose only job is letting an *outsider* replicate our numbers
against their own endpoint without our stack.

## The one hard rule

**We never depend on any opponent — Hermes, unsloth, a cloud provider — ever.** Optional
integrations that reach what a user already runs are an asset; a forced dependency is a
weakness (and off-grid: unmanageable by our daemons).

## Run OUR models — Rust `benchmark/*` commands (on-grid, persona-callable)

The benchmark catalog + runner are Rust (`core/continuum-core/src/commands/benchmark.rs`),
mirroring the model catalog. `benchmark/run` is a thin wrapper over `cognition/eval` — one
grader, never reimplemented.

```bash
cu benchmark/list                                              # the catalog (name, grader, tasks, runnable)
cu benchmark/run --persona_id <UUID> --name humaneval-rs --limit 40
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
