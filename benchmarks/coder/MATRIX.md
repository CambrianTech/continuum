# Coder benchmark matrix — humaneval-rs (rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`.
OURS = the same model through the full Continuum loop. opencode = the same model through the
opencode agentic harness (fairness shim recovers its narrated tool calls). Hermes-3-8B is a fixed
opponent. Identical weights across RAW/OURS/opencode.

| model | RAW one-shot | OURS (Continuum) | opencode | Hermes-3-8B |
|---|---|---|---|---|
| Qwen2.5-Coder-14B (20 tasks) | 90% (18/20) | **90% (18/20)** | 75% (15/20) | 52% (21/40) |
| Devstral-Small-24B (5 tasks) | 100% (5/5) | **100% (5/5)** | — | 52% (21/40) |
| qwen3.5-4b-code-forged (5 tasks) | — | **80% (4/5)** | — | 52% (21/40) |

On the matched 14B row: **OURS beats opencode by 15 pts and Hermes by 38, at zero tax vs raw.**

## Reproduce

```bash
# OURS + RAW for every model in the config (boot a Continuum core first):
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark humaneval-rs --limit 20 --out benchmarks/coder/MATRIX.md

# opencode cell needs its lane + shim up first:
llama-server -m <qwen14b.gguf> --port 8093 -c 32768 --jinja &
python3 benchmarks/coder/toolcall_shim.py --listen 8094 --upstream http://127.0.0.1:8093 &
# (opencode reads ~/.config/opencode/opencode.json → the shim; harness uses -m local/qwen14b)
```

Add a model = one row in `models.json`. A bigger machine sweeps more models, same command.
