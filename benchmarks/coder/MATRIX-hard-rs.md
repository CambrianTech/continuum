# Coder benchmark matrix — hard-rs (40 tasks, rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the same class of local model through the opencode agentic harness (fair tool-format shim).

| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |
|---|---|---|---|---|
| Hermes-3-Llama-3.1-8B | — | 12% (1/8) | — | — |
| Qwen2.5-Coder-1.5B | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-3B | — | 25% (2/8) | — | — |
| qwen3.5-4b-code-forged (OURS-forged) | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-14B | 0% (0/8) | 0% (0/8) | 0% (0/8) | +0% |
| Devstral-Small-24B | 38% (3/8) | 38% (3/8) | — | +0% |

## Reproduce

```bash
# boot a Continuum core (serves your local model), then:
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark hard-rs --limit 40 --out benchmarks/coder/MATRIX.md
```

Add a model = one row in `benchmarks/coder/models.json`. A bigger machine with more VRAM sweeps more models with the identical command.
