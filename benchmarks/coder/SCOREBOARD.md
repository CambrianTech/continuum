# Coder gym scoreboard — HumanEval-Rust (compile + run graded)

Reproduce with `run_ours.sh` (ours) and `oneshot_opponent.py` (opponents). Same tasks, same
grader. Opponents are external `/v1` endpoints — we never depend on any of them.

## 40-task slice (HumanEval_0 … HumanEval_39)

| model / system | passed | pass@1 | via | notes |
|---|---|---|---|---|
| **Qwen2.5-Coder-14B — OURS** | 35/40 | **88%** | Continuum system | our best-fit local coder, full stack |
| Hermes-3-Llama-3.1-8B | 17/40 | 42% | our system, one-shot serve | general model, not a code specialist |

*First local head-to-head (2026-07). Ours more than doubles Hermes on the coder gym with a
valid grader and zero inference errors on either side. The margin is dominated by model-fit
(specialist vs generalist) — that IS our thesis (pick/tune the best-fit local model, the move
cloud can't make), not a system-lift claim. Isolate system lift by running the SAME model both
one-shot and through `run_ours.sh`.*

## Pending

- **unsloth** — score via `oneshot_opponent.py` against an unsloth `/v1` gateway (external).
- **Hermes via external `/v1`** — re-measure one-shot through the standalone harness (fully
  decoupled from our serving) to confirm the number off the product entirely.
- **System-lift isolation** — same local model, one-shot vs `run_ours.sh`, to attribute the
  gap between model-fit and our loop/PX.
- **airc-node opponent** — reach a model over an airc peer's `/v1` (grid-encapsulation proof).
