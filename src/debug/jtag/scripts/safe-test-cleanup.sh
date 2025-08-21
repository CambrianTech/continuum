#!/bin/bash
# Safe Test Cleanup Script
# Safely cleans logs for the active example with comprehensive validation

set -euo pipefail

# Get the active example logs path
ACTIVE_EXAMPLE_PATH=$(npx tsx scripts/get-active-example-logs.ts | sed "s/\/server.log//" | sed "s/logs$/logs\/*/")

# Safety validation - multiple layers
if [ -z "$ACTIVE_EXAMPLE_PATH" ]; then
    echo "SAFETY: Empty path detected, aborting cleanup"
    exit 1
fi

# Must contain 'examples/' and '/logs/*' 
if [[ ! "$ACTIVE_EXAMPLE_PATH" == *"examples/"*"/logs/"* ]]; then
    echo "SAFETY: Path doesn't match expected pattern: $ACTIVE_EXAMPLE_PATH"
    echo "SAFETY: Expected pattern: */examples/*/logs/*"
    exit 1
fi

# Additional safety - must not be root or system directories
if [[ "$ACTIVE_EXAMPLE_PATH" == "/" ]] || \
   [[ "$ACTIVE_EXAMPLE_PATH" == "/home"* ]] || \
   [[ "$ACTIVE_EXAMPLE_PATH" == "/usr"* ]] || \
   [[ "$ACTIVE_EXAMPLE_PATH" == "/var"* ]] || \
   [[ "$ACTIVE_EXAMPLE_PATH" == "/etc"* ]]; then
    echo "SAFETY: Refusing to clean system directory: $ACTIVE_EXAMPLE_PATH"
    exit 1
fi

# Check if logs directory exists - if not, that's fine (fresh system)
if [ ! -d "$(dirname "$ACTIVE_EXAMPLE_PATH")" ]; then
    echo "✅ No logs to clean (fresh system): $(dirname "$ACTIVE_EXAMPLE_PATH")"
    exit 0
fi

# All safety checks passed - proceed with cleanup
echo "✅ SAFETY VALIDATED: Cleaning logs: $ACTIVE_EXAMPLE_PATH"
rm -rf $ACTIVE_EXAMPLE_PATH 2>/dev/null || true
echo "✅ Log cleanup complete"