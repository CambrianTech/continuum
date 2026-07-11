# Coder benchmark matrix — hard-rs (40 tasks, rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the same class of local model through the opencode agentic harness (fair tool-format shim).

| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |
|---|---|---|---|---|
| Hermes-3-Llama-3.1-8B | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-1.5B | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-3B | — | 25% (2/8) | — | — |
| qwen3.5-4b-code-forged (OURS-forged) | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-14B | — | 62% (5/8) | 0% (0/8) | — |
| Devstral-Small-24B ⚠ cell failed (see log) | — | — | — | — |

## Reproduce

```bash
# boot a Continuum core (serves your local model), then:
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark hard-rs --limit 40 --out benchmarks/coder/MATRIX.md
```

Add a model = one row in `benchmarks/coder/models.json`. A bigger machine with more VRAM sweeps more models with the identical command.

## gym-mine: real-repo bugfix tasks (bitflags, doubly-verified)

Tasks mined by `gym/mine` from the bitflags crate's history (bugfix-revert: setup re-breaks
`src/lib.rs` at the fixing commit's parent; DoD restores canonical tests and runs `cargo test`).
Three tasks, each verified broken-fails/fixed-passes at mining time. Solver = Asha
(Devstral-Small-2507 24B) through her LIVE cognition via `cognition/eval`, full act→observe
loop, honest room contention (personas stayed live — first-class citizens even during benchmarks).

| run | pass | acts (read/shell/tree/edit) | infra | verdict |
|---|---|---|---|---|
| run1 (2026-07-11) | 0/3 | 118 (58/54/6/0) | 58/65 reads killed by FALSE "Security: escapes" on in-sandbox ENOENT | INVALID — instrument lied (fixed: ae4c50127) |
| run2 (2026-07-11, post-fix) | 0/3 | 106 (12/76/18/0) | clean (0 false errors; 4 honest NotFound, recovered) | HONEST ZERO — diagnosed + self-verified 100%, never attempted a repair edit |

The run2 zero is the finding, not a failure of the harness: she reads the failing output,
re-runs the tests, narrates the diagnosis — and never converts to `code/edit`. Same
intention→action texture as the live room (#122's curriculum; Scenario Library
`narrate-vs-act` generator). Open instrument follow-ups before the next arm: prompt-captures
do not record the native `tools` array (cannot audit whether edit_file/write_file were
offered), and the repeat-guard nudge text ("I should ANSWER…") biases Speak over a
different act.
