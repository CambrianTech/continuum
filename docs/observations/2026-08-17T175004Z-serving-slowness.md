# Observation — serving slowness / citizen starvation

- **Captured (UTC):** 2026-08-17T17:50:04Z
- **Core build:** 08ccd6139
- **Method:** live probes + process table. NO prior cards consulted.

## Lane process
```
16152 /Users/joel/.continuum/bin/llama-server -m /Users/joel/.cache/huggingface/hub/models--ggml-org--Qwen3.8-27B-GGUF/snapshots/0669b98607d47046c7c2b3f801011d54a08cfccf/Qwen3.8-27B-Q4_K_M.gguf --alia
```

## Lane uptime (how long since it last restarted)
```
pid 16152  elapsed=16:53
```

## Slot state
```
task=8034 processing=True prefill=384/506 decoded=123 n_ctx=32512
```
