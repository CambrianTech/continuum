---
name: continuum:logs
description: Tail Continuum system + persona logs without leaving the IDE. Shows inference routing, persona cognition, IPC errors, memory pressure.
user-invocable: true
allowed-tools: Bash, Read
argument-hint: "[--persona <name> | --system | --routing | --errors | --all]"
---

# Continuum Logs

Stream or read logs from the running Continuum stack. Default: last 50 lines of system + routing. All output goes to the user's terminal via Claude Code.

## Log locations

```
~/.continuum/jtag/logs/
├── system/
│   ├── continuum-core.log          # Rust core: IPC, memory, startup
│   ├── modules/
│   │   ├── ai_provider.log         # Adapter selection: "Using X adapter for model Y"
│   │   ├── inference.log           # DMR catalog, model loading, tok/s
│   │   ├── data.log                # ORM operations, handle resolution
│   │   └── ...
│   └── personas_*_logs_tools.log   # Per-persona tool execution
├── prompt-captures.jsonl           # Full LLM request/response pairs
└── personas/<name>/cognition.log   # Per-persona thought process (if enabled)
```

## Default (system + routing — most useful for "why isn't chat working?")

```bash
LOGS=~/.continuum/jtag/logs/system
echo "=== AI Provider Routing (last 20) ==="
tail -20 "$LOGS/modules/ai_provider.log" 2>/dev/null || echo "No ai_provider.log"
echo ""
echo "=== Inference / DMR (last 10) ==="
tail -10 "$LOGS/modules/inference.log" 2>/dev/null || echo "No inference.log"
echo ""
echo "=== Errors (last 10) ==="
grep -h "ERROR" "$LOGS/modules/"*.log "$LOGS/continuum-core.log" 2>/dev/null | tail -10 || echo "No errors"
```

## --routing (adapter selection only)

```bash
grep "Using.*adapter for model" ~/.continuum/jtag/logs/system/modules/ai_provider.log 2>/dev/null | tail -30
```

Report: which adapter (docker-model-runner vs candle vs cloud) handled each request. If all say "candle" when DMR should be active, that's a routing bug.

## --errors (all errors across modules)

```bash
grep -rh "ERROR\|FATAL\|panic" ~/.continuum/jtag/logs/system/ 2>/dev/null | tail -30
```

## --persona <name> (e.g. --persona helper)

```bash
NAME=$1  # helper, teacher, codereview, local
echo "=== $NAME cognition ==="
cat ~/.continuum/jtag/logs/personas/$NAME/cognition.log 2>/dev/null | tail -30 || echo "No cognition log (enable: ./jtag logging/enable --persona=$NAME --category=cognition)"
echo ""
echo "=== $NAME tools ==="
cat ~/.continuum/jtag/logs/system/personas_${NAME}_logs_tools.log 2>/dev/null | tail -20 || echo "No tool log"
```

## --all (everything, verbose)

```bash
LOGS=~/.continuum/jtag/logs/system
for f in "$LOGS/modules/"*.log; do
  echo "=== $(basename $f) (last 5) ==="
  tail -5 "$f" 2>/dev/null
  echo ""
done
echo "=== Memory pressure ==="
grep "memory_pressure" "$LOGS/continuum-core.log" 2>/dev/null | tail -5
echo "=== MEMLEAK tracker ==="
grep "MEMLEAK" "$LOGS/continuum-core.log" 2>/dev/null | tail -5
```

## Docker container logs (when running containerized)

If the user's stack is Docker-based (not native npm start):

```bash
docker logs continuum-continuum-core-1 2>&1 | tail -30
docker logs continuum-node-server-1 2>&1 | tail -20
```

## Interpretation hints

When reporting log output to the user:
- `Using docker-model-runner adapter` = good, GPU inference active
- `Using candle adapter` = local CPU, check if DMR should be registered
- `DMR live model catalog: N model(s)` = DMR initialized, N models available
- `MEMLEAK RSS=XMB` = memory tracking, not necessarily a leak (check trend)
- `ERROR: No IPC connections` = node-server can't reach continuum-core
- `404` on model load = model not pulled into DMR (`docker model pull ...`)
