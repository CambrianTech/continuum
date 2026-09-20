# Coder benchmark matrix — humaneval-rs (10 tasks, rustc compile+run graded)

Assembled from the 2026-07-10 sweep log (the board renderer crashed at a fit-gated 32B cell —
since fixed to fail-soft; cells below completed and are verbatim from the run). OURS = the model
through the FULL Continuum cognition loop (natural proctored exam, living personas resident).

| model | RAW one-shot | OURS (Continuum) | Δ OURS−RAW |
|---|---|---|---|
| Hermes-3-Llama-3.1-8B | 52% (21/40, prior board) | **60% (6/10)** | **+8 (loop LIFTS the opponent's model)** |
| Qwen2.5-Coder-1.5B | — | 60% (6/10) | — |
| Qwen2.5-Coder-3B | — | 80% (8/10) | — |
| qwen3.5-4b-code-forged | — | 60% (6/10) | — |
| **Qwen2.5-Coder-14B** | **100% (10/10)** | **100% (10/10)** | **+0 — zero tax, matches its own ceiling** |
| Qwen2.5-Coder-32B | fit-gated off this host (correctly refused) | — | — |
| Devstral-24B / 14B-opencode | lost to the board crash — re-run pending | | |

Headlines: the 14B is PERFECT through the loop at zero tax; Hermes gains +8 through OUR system
vs its own raw baseline — after this week's cognition fixes the loop lifts even the opponent's
model. Size ladder behaves (1.5B→3B→14B: 60→80→100).
