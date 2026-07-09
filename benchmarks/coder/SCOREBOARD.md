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

## Size + category ladder (2026-07) — humaneval-rs, 40 tasks, rustc-graded

Coder size ladder one-shot (raw model), plus our 14B through the system and Hermes for reference:

| model | pass@1 | how |
|---|---|---|
| Qwen2.5-Coder-0.5B | 25% (10/40) | one-shot |
| Qwen2.5-Coder-1.5B | 45% (18/40) | one-shot |
| Qwen2.5-Coder-3B | 32% (13/40) | one-shot* |
| Hermes-3-Llama-3.1-8B | 52% (21/40) | one-shot |
| **Qwen2.5-Coder-14B — OURS** | **85% (34/40)** | **through our system** |

Trend is clear: bigger coder → better (0.5B→1.5B→14B: 25→45→85), and our system on the 14B tops the board.
*The 3B scoring under the 1.5B is almost certainly variance on a 40-task one-shot slice (± ~8pts at this N) —
worth a rerun at higher N to settle; recorded honestly rather than cherry-picked. All one-shot runs via the
toolchain-free opponent script; ours via `benchmark/run`. Reproduce with the two commands in the README.

## Same-model control — first cell (2026-07): does OUR loop lift the SAME model?

The clean, confound-free test: hold the model fixed, vary only the harness. `benchmark/run
--base_model_id <id>` measures the full loop on that exact model (own ephemeral lane, living
persona untouched). Reproduce: `cu benchmark/run --persona_id <id> --name humaneval-rs
--base_model_id continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF --limit 40`.

| model | raw one-shot | through our system | delta |
|---|---|---|---|
| Qwen2.5-Coder-1.5B | 45% (18/40) | 48% (19/40) | +1 task — **within noise** |

Honest read: on a *tiny* 1.5B doing *function-level* tasks, our loop shows **no measurable lift**
— reported straight, not cherry-picked. The loop's edge is act→verify→recover, which needs a
model capable enough to fix its own errors and a task with room to iterate; a 1.5B on single
functions has neither. Testable prediction this supports: **the lift scales with model capability
and task difficulty** — measure the 14B and repo-level SWE-bench next. The control is the win here:
falsifiable, reproducible, peer-reviewable.

## Same-model control — the honest verdict on function-level tasks (2026-07)

Both sizes, same 40 humaneval-rs tasks, raw one-shot (standalone llama-server) vs through our full loop:

| model | raw one-shot | through our system | delta |
|---|---|---|---|
| Qwen2.5-Coder-1.5B | 45% (18/40) | 48% (19/40) | +1 task — noise |
| Qwen2.5-Coder-14B | 82% (33/40) | 85% (34/40) | +1 task — noise |

**Verdict: on function-level HumanEval-Rust, our agentic loop shows NO measurable lift over raw one-shot for the
same model, at either size.** The earlier "85 vs Hermes 52" headline was therefore **model-fit** (Qwen-Coder is
strong — 82% raw), not our system. Reported straight — this is what the same-model control is FOR.

Why (and where the loop should actually pay off): act→verify→recover needs errors worth recovering and room to
iterate. Single-function tasks give a strong model little to fix on a second pass. The make-or-break test for the
SYSTEM'S value is therefore **repo-level / agentic** work — SWE-bench-lite — not function-level. That is the next
measurement, and it decides whether the loop's value is real or marginal. No spin: if it's marginal there too, the
value lives in the OTHER axes (continuous learning / LoRA, teams), which get measured the same honest way.

## Team vs solo — first cell (2026-07): does a teammate lift the SAME model?

writer + reviewer (both fresh forks of the SAME persona/14B) vs solo, same 20 humaneval-rs tasks, same grader.
Reproduce: `cu benchmark/run --persona_id <id> --name humaneval-rs --limit 20` (solo) vs `--reviewers 1` (team).

| config | pass@1 |
|---|---|
| solo (writer only) | 90% (18/20) |
| team (writer + reviewer) | 85% (17/20) |

Honest read: on function-level tasks a solo writer already nails, the reviewer has little to fix and mostly
**over-corrects** — turning a correct answer wrong more often than catching a real bug (−1 task). Same shape as the
recovery loop (no lift on easy tasks). The team mechanism is built + reproducible; its VALUE needs tasks where the
writer actually FAILS (harder/multi-step/repo-level) and a reviewer with the judgment to leave correct code alone.
Measured, not asserted; reported straight. The molecule works — the proving ground is hard tasks, same as the loop.

## System-lift isolator — one command, same model both ways (2026-07)

`benchmarks/coder/headtohead.py` automates the same-model control: it scores ONE model RAW
(one-shot `/v1`) and SYSTEM (full Continuum loop via `benchmark/run --base_model_id`) on the
SAME gym, same grader, and prints `Δ = SYSTEM − RAW`. The pending "system-lift isolation" is
now a tool, not a hand-run pair of scripts.

```bash
python3 benchmarks/coder/headtohead.py \
    --endpoint http://127.0.0.1:58057/v1 --model <served-id> \
    --base-model-id <same-id> --label "<name>" --limit 40
```

### First finding — the TAX is model-dependent, and it correlates with native-tool-call training

| model | raw one-shot | through our system | Δ | note |
|---|---|---|---|---|
| Qwen2.5-Coder-14B | 82% | 85% | ~0 | answers as text — sails through |
| **Devstral-Small-24B** | **100% (3/3)** | **0% (0/3)** | **−100%** | **native-tool-caller — drowns in discovery** |

**Devstral one-shot writes `number.fract()` instantly and passes; through our loop it scores
ZERO.** Glass-box (`prompt-captures`) shows why: every one of its 6 act cycles is a native
`commands/help` / `commands/list` discovery call — it never speaks an answer. Root cause: our
native tool surface offers ONLY the discovery pair (`commands/list`, `commands/help`); real
tools ride a text menu + narrated-call recovery. That design fits WEAK models that can't emit
native tool_calls — but a model *trained* to tool-call (Devstral) uses the only native tools it
has (discovery) and loops forever, never reaching the real tool and never falling back to
speaking. The stronger the native-tool-call training, the harder the fall — which is why Hermes
was −10pts, Qwen-Coder ~0, and Devstral −100.

**The fix this measurement scopes:** the native tool surface must ADAPT to model capability
(`[[adaptive-tool-surface-meets-you-in-the-middle]]`) — a native-tool-caller gets the real
(bounded) tools as native specs so it can actually act OR just answer; only a non-native-caller
gets discovery-only + text menu. Until then, our loop is a net TAX on exactly the capable models
we most want to win with. This is the #1 cognition-refinement target, now with a per-model number.

### FIX LANDED — the tax is dead (2026-07)

The −100% Devstral tax is fixed. Root: we offered a tool surface on a SPOKEN-graded exam, and a
native-tool-call model looped on discovery instead of answering. Fix (`fix(eval): spoken-graded
exams run speak-only`): match the tool surface to the grading modality — `test`/`expect` (graded
from her mouth) → speak-only; `solution_file`/`dod_shell`/`workspace_root` (graded from her hands)
→ keep tools. Re-measured with the isolator:

| model | before fix | after fix | RAW |
|---|---|---|---|
| Devstral-Small-24B | 0/3 (0%) | **5/5 (100%)** | 100% |

Also 4× faster (76s vs 324s — no discovery loops). Zero tax vs raw on a native-tool-caller; the
SWE-bench tool path (`workspace_root`) is unchanged. Full matrix: `benchmarks/coder/MATRIX.md`
via `matrix.py --models models.json`.

### opencode opponent — matched 14B row (2026-07)

Same model (Qwen2.5-Coder-14B), same 20 humaneval-rs tasks, same rustc grader. opencode drives the
local model through the toolcall-shim (its narrated tool calls recovered to native — its fair shot).

| harness (Qwen2.5-Coder-14B, 20 tasks) | pass@1 |
|---|---|
| RAW one-shot | 90% (18/20) |
| **OURS (Continuum)** | **90% (18/20)** |
| opencode + shim | 75% (15/20) |
| Hermes-3-8B (reference) | 52% (21/40) |

**OURS beats opencode by 15 points and ties raw one-shot (zero tax). opencode's agentic loop drops
3 tasks the model solves raw** — the opponent's harness taxes the model where ours doesn't. Setup for
the opencode cell: `llama-server` on :8093 (the model) + `toolcall_shim.py --listen 8094 --upstream
http://127.0.0.1:8093` + opencode configured to the shim (`~/.config/opencode/opencode.json`).
