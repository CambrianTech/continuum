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

## Decoupled cross-check (external /v1, one-shot, off our product entirely)

| model / system | passed | pass@1 | via | notes |
|---|---|---|---|---|
| Hermes-3-Llama-3.1-8B | 21/40 | 52% | standalone llama-server, one-shot | clean prompt, `oneshot_opponent.py` |

**Kink found (the cognition-refinement target):** Hermes scores **52% raw one-shot** but only **42% through our
system** — our stack *hurt* an unfamiliar model by ~10pts. Our ~4.6K-token tool menu + grounding context distracts
a model not tuned to it; our own Qwen-Coder-14B eats that context fine (88%). So our system is **not yet a universal
lift** — it lifts the model we tuned around and taxes a stranger. Closing that (concise/adaptive context per task,
and the act-reflex LoRA) IS the "win for every model out of the box" work.

## Head-to-head, run through the system (2026-07)

Ours via the Rust `benchmark/run` command; Hermes via the toolchain-free `oneshot_opponent.py`.
Same 40 HumanEval-Rust tasks, same rustc compile+run grader.

| model | pass@1 | via |
|---|---|---|
| **Qwen2.5-Coder-14B — OURS** | **85% (34/40)** | `cu benchmark/run --name humaneval-rs` |
| Hermes-3-Llama-3.1-8B | 52% (21/40) | external /v1, one-shot |

**+33 points, run through the actual benchmark system, cross-validated (Hermes 52% a third time).**
Reproduce: `cu benchmark/run --persona_id <UUID> --name humaneval-rs --limit 40` for ours; bring up
any `/v1` and `python3 benchmarks/coder/oneshot_opponent.py --endpoint … --limit 40` for a challenger.
