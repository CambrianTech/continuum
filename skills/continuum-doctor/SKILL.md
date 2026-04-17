---
name: continuum:doctor
description: Diagnose Continuum install + runtime problems — submodules, IPC sockets, GPU backend, DMR routing, disk space, model presence.
user-invocable: true
allowed-tools: Bash
argument-hint: ""
---

# Continuum Doctor

Run the diagnostic, read the output, name the root cause — don't just relay it.

## Run

```bash
continuum doctor
```

The CLI checks: submodules initialized, IPC sockets present, backend cuda-vs-cpu, scheduler-vs-llama-server, cloud keys, disk free, DMR reachability.

## Interpret + narrow the root cause

The output usually has multiple ✓ and one or two ✗ / ⚠. Focus the user on what actually matters:

**Common patterns you'll see + the right remediation prose:**

- **`DMR backend: latest-cpu`** (Mac or Linux+Nvidia with GPU present) → "Docker Desktop → Settings → AI → check 'Enable GPU-backed inference'. Without this, inference runs on CPU even with a GPU. Then `continuum update` to refresh."

- **`Host-side TCP: closed`** (continuum-core can't reach DMR) → "Docker Desktop → Settings → AI → check 'Enable host-side TCP support' (port 12434). Without this, containers can't reach DMR."

- **`Qwen3.5 not in DMR catalog`** → "Run `docker model pull hf.co/continuum-ai/qwen3.5-4b-code-forged-GGUF` — this is what the default personas route to. Install should have done this but on re-runs it can skip."

- **`Submodules not initialized`** → "Run `git submodule update --init --recursive` from the repo root. Usually happens when the repo was downloaded as a ZIP instead of cloned."

- **`IPC socket not present: /root/.continuum/sockets/continuum-core.sock`** → "continuum-core hasn't started or crashed. Check `continuum logs continuum-core` for the error. Classic: missing CUDA toolkit, OOM at model load, or port binding conflict."

- **`Disk free < 10GB`** → "Low disk; model pulls + docker layer cache will fail. Prune with `docker system prune -a` and reconsider which variants you need."

- **`AIProviderDaemon: stuck N seconds since last success`** → "Usually a FALSE positive if chats are working — it's a heartbeat metric, not a real failure. Verify by sending a chat. If chats ALSO hang, then it's real."

## When there's nothing to diagnose

If everything's green, say so plainly: "All checks pass. If you're still hitting a problem, describe the user-facing symptom (what the widget shows, what chat does) — I can look at that angle."

## Related

- `/continuum:update` — re-pull images if version mismatch is the cause
- `/continuum:status` — see what's currently running
- `docs/SETUP.md` → per-OS sections — the failure modes are documented there in `if X then Y` shape

## Notes

The CLI's `doctor` output is designed to be machine-parseable AND human-readable. Your job is to cut through the wall of checks and surface the ONE thing the user probably cares about. Never say "I see several issues" without naming which matters — that's useless.
