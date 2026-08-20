# Harness Runbook — the straightforward, repeatable cognition-optimization loop

> **Why this file exists** (Joel, 2026-06-28): *"We will want to repeatedly and
> easily capture the data we need so we can iterate here on perfection, and so an
> amnesiac version of yourself doesn't have to get lost because our process isn't
> straightforward."* and *"Rarely do I see anyone start anew … and really understand
> the big picture."*
>
> This is the **operational** companion to the design docs. If you are a fresh
> instance with no context, **start here**, then run the one command. The big-picture
> design lives in — and is not restated here (compression principle):
> - [PERFORMANCE-HARNESS-FRAMEWORK.md](PERFORMANCE-HARNESS-FRAMEWORK.md) — the VDD-record schema + per-part performance covenants.
> - [OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md) — the CaptureSink pattern (Noop default at zero cost, Jsonl glass box for replay).
> - [COGNITION-VDD-TDD-HARNESSES.md](COGNITION-VDD-TDD-HARNESSES.md) — record/replay of a persona turn.
> - [INFERENCE-LANES-REALISTIC.md](INFERENCE-LANES-REALISTIC.md) — the lane model the latency levers live on.

## The big picture in five sentences

We are tuning a **society of personas** (resident minds, each a `WorkspaceCycle`) so
that **many of them, simultaneously, answer fast and well** on a user's heterogeneous
grid. The loop is the scientific method made cheap: **measure where the time and the
errors go → attack the single greatest inefficiency → change ONE variable → re-measure
against the committed baseline → keep it only if the number moved.** Every load-bearing
decision is captured to a glass-box JSONL so the *next* run — and the next amnesiac
instance — can see exactly what happened without re-deriving it. The persona is measured
on a **forked copy** (humane snapshot-eval, #59) so iterating never degrades the living
citizen. The endgame is not just faster inference but **personas that learn to get
better** — the genome loop (#32) trains a LoRA, and the same harness proves the lift.

## The one command

```bash
# from the repo root, with the headless core already running:
tools/scripts/harness/cognition-cycle.sh --persona Asha --note "what I changed this run"
```

That script (read its header — it is the authoritative procedure) does, idempotently,
failing loud at the first missing precondition:

1. resolves the `continuum` client (prints the build command if absent),
2. confirms the core answers `ping` on its IPC socket,
3. resolves the persona by name or UUID (`cognition/personas`),
4. snapshots every glass-box stream's length,
5. runs `continuum cognition/eval` (single-pass, or A/B with `--gene`),
6. deltas the streams and collects *this run's* new capture lines,
7. writes a timestamped report dir and prints the headline record.

Output: `~/.continuum/harness-runs/<stamp>-<persona>/report.md` (+ `eval.json` +
`placement-decisions.jsonl`). Use `--dry-run` to validate the whole chain without
spending any inference.

### Bringing the world up from cold (only if `ping` fails)

```bash
export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"   # the ONE shared cache
# 1. build + run the headless core (pure Rust — never npm/jtag):
bash tools/scripts/start-server.sh                              # binds /tmp/continuum-core.sock
# 2. build the continuum client once:
cargo build --manifest-path core/continuum-core/Cargo.toml --bin continuum --features metal,accelerate
# 3. confirm a persona is online (they resume from disk at boot):
$CARGO_TARGET_DIR/debug/continuum cognition/personas
#    none online? spawn one:  continuum persona/instances/bootstrap
```

## The iteration loop (what "iterate here till it's pretty good" means)

```
        ┌─────────────────────────────────────────────────────────┐
        │  cognition-cycle.sh  →  report.md + glass-box JSONL       │
        └───────────────┬─────────────────────────────────────────┘
                        │  read: pass_rate, lift, mean/p95 latency, tok/s, device
                        ▼
        Where is the time / the error going?  (capture streams below)
                        │
                        ▼
        Pick the SINGLE greatest inefficiency  (lever ladder below)
                        │
                        ▼
        Change ONE variable  →  rebuild if Rust  →  re-run the SAME eval set
                        │
                        ▼
        Compare to the prior line in the progress ledger. Kept iff the number moved.
```

**Locked-variable discipline:** change one thing per cycle. The eval set and persona are
the controls; the lever is the independent variable; the report is the measurement. A
run that changes two things proves nothing.

## Capture-stream map — the data we "easily capture", and the question each answers

All live under `~/.continuum/fixtures/` (auto-enabled when the dir exists; Noop and
zero-cost otherwise). Each is one JSON object per line, schema-versioned.

| Stream | Path | Defined in | Answers |
|---|---|---|---|
| **Prompt captures** | `prompt-captures/<persona>.jsonl` | `cognition/prompt_capture.rs` | "What exact tokens was she fed, and what did she emit, per agent-loop iteration?" |
| **Workspace traces** | `workspace-traces/<persona>.jsonl` | `cognition/workspace_capture.rs` | "Which faculty won attention, what did each bid, and **how many µs did each take**?" (per-faculty timing) |
| **Placement decisions** | `placement-decisions/decisions.jsonl` | `inference/placement_capture.rs` | "Did the lane run on **GPU or CPU**, and why? How much VRAM was free?" |
| **Progress ledger** | `~/.continuum/progress/<persona>.jsonl` | `cognition/eval.rs` | "pass_rate / latency / tok-s **over time** — is the trend up?" |

To read the most recent run's streams: open the `report.md` in the run dir; it already
deltas them. For the live tail of a stream: `jq . <path> | tail`.

## Where the time goes — the lever ladder (greatest inefficiency first)

Joel's framing: the grid is an **asset-streaming render engine**; treat every avoidable
recompute as *"rasterization as a great sin."* Carmack-level efficiency wins even on a
GPU. The levers, in rough order of leverage for *many simultaneous personas*:

1. **GPU-first placement** *(DONE — `ea7f96e4e`)*. Never let a lane idle the accelerator
   on a sizing miss. The `lane_placement` field on every eval result + the
   placement-decisions glass box make a CPU spill **visible**, never a silent 4-tok/s
   collapse. Next: partial `--n-gpu-layers N` offload owned by the `ResourceGovernor`
   (#56) — fill the GPU to the brim, spill only the residue.
2. **Prefix / KV reuse** *(named next lever)*. The system prompt + standing grounding is
   re-encoded every turn — a static asset re-rasterized per frame. llama-server slot
   caching reuses the KV prefix across turns. Pure speed, a clean locked-variable A/B.
3. **Continuous batching across persona lanes**. N personas deciding at once should share
   forward passes through one base model, not serialize. This is *the* lever for the
   "many simultaneous personas" target — see INFERENCE-LANES-REALISTIC.md.
4. **Warming**. Cold lane spawn pays model-load on the first turn. Keep the hot set
   resident; page LoRAs (genome), not base weights — texture-streaming, not reload.
5. **Genome learning** *(#32)*. The error bands the gym surfaces (e.g. the recurring
   `T: Ord` trap in `CODER-EVAL-BASELINE.md`) are closed by **training**, not by
   knobs — and the same harness proves the `lift`. This is "they should learn to get
   better."

When you find a *new* sink, add it here with its lever. This ladder IS the running
record of where the inefficiency is.

## Failure signatures (battle-tested — read the glass box, don't guess)

The harness's first real run (2026-06-28) caught one immediately. Catalogue them here as
you meet them so the next instance recognizes the shape in seconds, not hours:

| Signature in `report.md` | Confirm via | Root cause | Fix |
|---|---|---|---|
| `pass_rate` craters to ~0 **and** `total_output_tokens` tiny (≈ 2/task), every task "no match", reply is the bare word `"PASS"` | `jq .response.content prompt-captures/<id>.jsonl \| tail` shows the reply is literally `"PASS"`; the system prompt contains the `[Silence Option]` block | The persona **took the silence escape**, not a lane failure. The `[Silence Option]` block (reply `PASS` to decline) was offered on a DIRECTED exam turn; a coder model declines instead of attempting → 0/13. Proven by A/B: same lane, strip the block → full answers. **`--embeddings` mode is a RED HERRING** — that lane generates fine; don't chase it. | Confirm the eval pins `directed=true` (withholds the silence affordance via `Workspace.directed_at_self`, fixed e49ae207e). If you truly suspect a degraded lane, `jq .response.usage` for `outputTokens` AND curl the lane directly with a real prompt before blaming `--embeddings`. |
| `lane_placement: null` in the report | the running core's start time predates your code change | You measured against a **stale core** — the new fields/glass-box aren't in that binary. | Rebuild + restart the core, then re-run. |
| `device: cpu` with `tok/s` ≈ 4 | `placement-decisions/decisions.jsonl` `reason` field | GPU-first spilled to CPU (sizing miss / no backend). | Read the `reason`; free VRAM (evict a resident lane) or fix the footprint estimate. |

The discipline the table encodes: **the report number tells you *something* is wrong; the
glass box tells you *what*.** Always open the capture stream before theorizing.

## If you are an amnesiac instance: resume here

1. Read this file's "big picture" + run `cognition-cycle.sh --dry-run` to see the live
   state (core up? persona online?).
2. Read the last few lines of `~/.continuum/progress/<persona>.jsonl` — that is where
   the metric stood and what the last change was (`note`).
3. Read the most recent `~/.continuum/harness-runs/*/report.md`.
4. Pick the top unaddressed lever above, change ONE variable, run the cycle, compare.
5. The standing constraints (no fallbacks/fail-loud, no heuristics steering cognition,
   Rust-only, validate via `continuum` never npm/jtag) are in `CLAUDE.md` and the memory index.

## Cross-references

- Script: `tools/scripts/harness/cognition-cycle.sh`
- Eval engine + result schema: `core/continuum-core/src/cognition/eval.rs`
- Baseline snapshots: `docs/genome/CODER-EVAL-BASELINE.md`
- Placement glass box: `core/continuum-core/src/inference/placement_capture.rs`
