# MoE serving measurement harnesses

Dev harnesses for measuring how a large MoE actually behaves when it does not fit in VRAM. **Not the
product path** — the governed path is the Rust serving daemon, which chooses residency division and
cache budget through the governor. What lives here is the instrumentation that produces the response
surface the governor learns from.

These started as untracked `scratch-*.sh` in the repo root with absolute paths to one operator's
machine baked in. That is fine while a prototype is still earning its keep and wrong the moment it
starts producing numbers people act on: nobody else could reproduce a single measurement. Everything
here now resolves from the environment and fails loud when it cannot.

## Setup

```bash
export CONTINUUM_MODELS_DIR=/path/to/gguf          # required if not ~/.continuum/models
export CONTINUUM_LLAMA_ENGINE=/path/to/llama-server # required if not ~/.continuum/bin/llama-server-k3
export CONTINUUM_CUDA_DIR=/path/to/cuda            # if cublas is not already on PATH
export CONTINUUM_WIN_BUILD_ENV=/path/to/env.sh     # Windows only: MSVC + CUDA environment
```

`common.sh` documents every variable, including the model glob, port, and output directory.

## Serve once and report

```bash
tools/dev/moe/serve.sh                 # default: all experts streamed, 24 GB device cache
tools/dev/moe/serve.sh 48 0            # baseline: streaming only, cache OFF
tools/dev/moe/serve.sh 34 0            # residency lever: 14 expert layers pinned resident, cache OFF
```

Prints the cache clamp, hit rate, decode tok/s, and the generated text — the last one matters, because
a residency bug shows up as incoherent output long before it shows up as a bad number.

## Sweep an axis into JSONL

```bash
tools/dev/moe/sweep.sh division 48 40 34 30 26   # expert layers left on CPU
tools/dev/moe/sweep.sh budget   0 6 12 18 24     # device-resident expert cache, GB
CONTINUUM_MOE_SWEEP_REPS=3 tools/dev/moe/sweep.sh budget
```

Each line of the resulting `.jsonl` is one `(setting, measured decode tok/s)` observation — the shape
`DivisionBandit.observe()` consumes, and the curve `CoverageModel` warm-starts from.

### Measurement discipline

**A single point is not a measurement.** Repeated *identical* configurations on V4-Flash varied by
~35% run to run, because decode rate tracks whether a given token's experts happened to be resident.
Establish the noise floor with `CONTINUUM_MOE_SWEEP_REPS` before believing any difference between two
points. A 20% gap inside a 35% spread is not a result.

Two things the harness deliberately does *not* do:

- **Never records 0 for a missing measurement.** No decode line means the request never decoded; that
  is written as `"status":"no_measurement"`, because a zero would look like a real slow point.
- **Never hides a boundary.** OOM or a load failure is recorded as a point with a `status`, since where
  the card runs out is exactly what the governor needs to know.

## `ds4-container-build-run.sh`

Runs *inside* an `nvidia/cuda` devel container: builds antirez/ds4 for the target arch and does a
one-shot generation. Reference implementation for comparison, not part of our serving path. Mounts
`/ds4` (source) and `/models` (GGUF).
