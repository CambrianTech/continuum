# Coder benchmark matrix — humaneval-rs (5 tasks, rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the same class of local model through the opencode agentic harness (fair tool-format shim).

| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |
|---|---|---|---|---|
| Devstral-Small-24B | 100% (5/5) | 100% (5/5) | — | +0% |
| Qwen2.5-Coder-14B | — | 100% (5/5) | — | — |
| qwen3.5-4b-code-forged (OURS-forged) | — | 80% (4/5) | — | — |

## Reproduce

```bash
# boot a Continuum core (serves your local model), then:
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark humaneval-rs --limit 5 --out benchmarks/coder/MATRIX.md
```

Add a model = one row in `benchmarks/coder/models.json`. A bigger machine with more VRAM sweeps more models with the identical command.
