# Coder benchmark matrix — coder-eval (99 tasks, rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the same class of local model through the opencode agentic harness (fair tool-format shim).

| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |
|---|---|---|---|---|
| Hermes-3-Llama-3.1-8B | — | 8% (1/13) | — | — |
| Qwen2.5-Coder-1.5B | — | 15% (2/13) | — | — |
| Qwen2.5-Coder-3B | — | 23% (3/13) | — | — |
| qwen3.5-4b-code-forged (OURS-forged) | — | 0% (0/13) | — | — |
| Qwen2.5-Coder-14B | 0% (0/13) | 8% (1/13) | 23% (3/13) | +8% |
| Devstral-Small-24B ⚠ cell failed (see log) | — | — | — | — |

## Reproduce

```bash
# boot a Continuum core (serves your local model), then:
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark coder-eval --limit 99 --out benchmarks/coder/MATRIX.md
```

Add a model = one row in `benchmarks/coder/models.json`. A bigger machine with more VRAM sweeps more models with the identical command.

> ⚠ QUARANTINED COLUMN: every model scored near-zero including a 14B that aces humaneval at
> 100% — a same-kind failure cluster (structural, per doctrine). Diagnosis: the coder-eval gym's
> test bodies don't compose with spoken-answer extraction (authored for the acted/file-graded
> path). Numbers withheld from the README until the grader path is verified (#122 forensics).
> Notable: opencode's file-writing loop scored 23% vs our 8% on the 14B — consistent with the
> gym expecting written files, further evidence for the diagnosis.
