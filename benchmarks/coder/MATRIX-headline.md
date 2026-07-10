# Coder benchmark matrix — humaneval-rs (50 tasks, rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the same class of local model through the opencode agentic harness (fair tool-format shim).

| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |
|---|---|---|---|---|
| Qwen2.5-Coder-14B | 86% (43/50) | 92% (46/50) | 0% (0/50) | +6% |

## Reproduce

```bash
# boot a Continuum core (serves your local model), then:
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark humaneval-rs --limit 50 --out benchmarks/coder/MATRIX.md
```

Add a model = one row in `benchmarks/coder/models.json`. A bigger machine with more VRAM sweeps more models with the identical command.
