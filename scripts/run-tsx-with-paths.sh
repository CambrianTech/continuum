#!/bin/bash
# Run tsx with JTAG TypeScript path mappings
# Usage: ./scripts/run-tsx-with-paths.sh <script.ts> [args...]

set -e

SCRIPT_PATH="$1"
shift # Remove first argument (script path)

# Navigate to project root
cd "$(dirname "$0")/.." 

echo "🔧 Running tsx with JTAG path mappings: $SCRIPT_PATH"

# Use tsx with explicit tsconfig from JTAG directory
npx tsx --tsconfig src/debug/jtag/tsconfig.json "$SCRIPT_PATH" "$@"