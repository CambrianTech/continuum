#!/bin/bash
# Check all 256 unused commands for implementations in the codebase
# Outputs: COMMAND | HAS_IMPL(1/0) | MATCHES

COMMANDS=(
    "admit-inbox-message" "cache-message" "check-adequacy" "check-content-dedup" 
    "check-redundancy" "classify-domain" "configure-rate-limiter" "create-engine"
    "dream-now" "embed-tools" "enqueue-message" "eval" "eval-status" "forget-context"
    "full-evaluate" "generate-recipe" "generate-response" "genome-activate-skill"
    "genome-coverage-report" "genome-evict-under-pressure" "genome-record-activity"
    "genome-state" "genome-sync" "get-state" "gpu-budget" "has-evaluated"
    "mark-evaluated" "observe" "personas" "plan-turn-batch" "playback" "prompt"
    "rate-proposals" "recall-engrams" "record-content" "redact-memory"
    "register-domain-keywords" "replay" "replay-request" "resolution-bench"
    "respond" "score-interaction" "select-model" "semantic-search-tools"
    "set-sleep-mode" "should-respond" "sync-adapters" "sync-domain-classifier"
    "track-response" "validate-response-decision" "vision-describe"
    
    # persona (16)
    "allocate" "catalog" "drain-turn-frame" "identity/set" "instances/despawn"
    "instances/get" "instances/hold" "instances/wake" "live-state" "rag-inspect"
    "reassign-model" "roster" "spawn" "turn-execute" "wall/list" "wall/pin"
    
    # ai (14)
    "inference/close" "inference/generate" "inference/inspect" "inference/load"
    "inference/models" "inference/open" "inference/status" "inference/unload"
    "lora/capabilities" "lora/list" "model-info" "models/list" "providers/health"
    "providers/list"
    
    # system (14)
    "cpu" "docker-tier-stats" "launch-mode/get" "launch-mode/set" "memory"
    "memory-budget" "memory-gate" "pressure" "reclaim-build-residue" "resources"
    "shutdown" "shutdown-commit" "shutdown-drain" "shutdown-target"
    
    # genome (13)
    "job-cancel" "job-pause" "job-resume" "job-status" "list" "pull" "push"
    "recall/replay" "sharing" "teach" "teach-status" "training-trigger/resolve"
    "training-trigger/return"
    
    # benchmark (12)
    "dispatch" "list" "pause" "resume" "round" "round-report" "round-stop"
    "rounds" "runs" "scoreboard" "validate" "verify"
    
    # code (8)
    "cargo/check" "cargo/test" "delete" "diff" "github/issue-create" "glob"
    "history" "shell-kill"
    
    # migration (7)
    "cutover" "pause" "resume" "rollback" "start" "status" "verify"
    
    # agent (6)
    "list" "solve" "start" "status" "stop" "wait"
    
    # auth (6)
    "oauth/providers" "oauth/refresh" "oauth/register" "oauth/revoke" "oauth/start"
    "oauth/status"
    
    # dataset (6)
    "from-captures" "from-turns" "import-csv" "import-realclasseval" "info" "list"
    
    # gpu (6)
    "eviction-candidates" "eviction-registry" "pressure" "register-consumer"
    "set-budget" "unregister-consumer"
    
    # memory (6)
    "append-event" "consciousness-context" "import" "recall-hook" "remember" "share"
    
    # models (6)
    "capabilities" "discover" "list" "pull" "remove" "try"
    
    # serving (6)
    "cache-probe" "load" "pin" "plan" "unload" "unpin"
    
    # vision (6)
    "cache-evict" "cache-stats" "cache-warm" "description-get" "description-put"
    "description-status"
    
    # data (5)
    "clear-all" "collection-stats" "list-collections" "read" "truncate"
    
    # plasticity (5)
    "analyze" "compact" "compress" "pipeline" "topology"
    
    # tool-parsing (5)
    "correct" "decode-name" "encode-name" "parse" "register-tools"
    
    # vector (5)
    "backfill" "index" "invalidate-cache" "search" "stats"
    
    # runtime (4)
    "list" "metrics/all" "metrics/module" "metrics/slow"
    
    # perception (3)
    "hot-edit" "interact" "session-close"
    
    # ports (3)
    "lease" "list" "release"
    
    # access (2)
    "get" "set"
    
    # airc (2)
    "queue-scan" "realtime-replay"
    
    # command (2)
    "migrate" "new"
    
    # debug (2)
    "probes/query" "prompt-reuse"
    
    # embedding (2)
    "groups" "similar"
    
    # events (2)
    "declare-class" "list-classes"
    
    # focus (2)
    "mute" "nudge"
    
    # inbox (2)
    "create" "drain-frame"
    
    # inference (2)
    "capacity" "llm/request"
    
    # keys (2)
    "list" "set"
    
    # log (2)
    "write" "write-batch"
    
    # nav (2)
    "close" "mark-read"
    
    # tool (2)
    "conformance" "usage"
    
    # singles
    "activity/state" "adapter/info" "avatar/snapshot" "capacity/io-probe"
    "coursework/import" "desktop/desktop" "generate/module" "get-stats/get-stats"
    "governor/status" "gym/mine" "health-check/health-check" "interface/screenshot"
    "mcp/refresh" "rag/compose" "recipe/run" "search/list" "vdd/report"
)

echo "=== DEAD COMMAND AUDIT: Implementation Check ==="
echo ""

dead=0
has_impl=0
for cmd in "${COMMANDS[@]}"; do
    # Convert hyphenated name to snake_case for Rust search
    snake=$(echo "$cmd" | tr '-' '_')
    
    # Search for the command (try both versions)
    matches=$(grep -rl "struct $snake\|impl.*$snake\|$cmd\|\"$cmd\"" --include="*.rs" 2>/dev/null | grep -v "^docs/" | head -5)
    
    if [ -n "$matches" ]; then
        count=$(echo "$matches" | wc -l)
        echo "HAS_IMPL: $cmd ($count files)"
        has_impl=$((has_impl + 1))
    else
        echo "DEAD:      $cmd"
        dead=$((dead + 1))
    fi
done

echo ""
echo "=== SUMMARY ==="
echo "Total commands checked: ${#COMMANDS[@]}"
echo "Has implementation:     $has_impl"
echo "Dead (no impl):         $dead"
