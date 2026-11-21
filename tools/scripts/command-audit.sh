#!/usr/bin/env bash
# Command audit: extract all registered commands with their source locations and families
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../" && pwd)"
cd "$REPO_ROOT"

echo "=== COMMAND AUDIT ==="
echo ""

# 1. Extract register_command! invocations (stateful)
echo "--- Stateful commands (register_command!) ---"
grep -rn 'register_command!' --include="*.rs" | \
  grep -vE '(//|//!|\$cmd|<Type>|MyCmd|MyCommand)' | \
  sed 's/.*register_command!(\([^)]*\)).*/\1/' | sort | uniq > /tmp/stateful_commands.txt

echo "Count: $(wc -l < /tmp/stateful_commands.txt)"
cat /tmp/stateful_commands.txt
echo ""

# 2. Extract register_stateless_command! invocations (stateless)
echo "--- Stateless commands (register_stateless_command!) ---"
grep -rn 'register_stateless_command!' --include="*.rs" | \
  grep -vE '(//|//!|\$cmd|<Type>|MyCmd|MyCommand)' | \
  sed 's/.*register_stateless_command!(\([^)]*\)).*/\1/' | sort | uniq > /tmp/stateless_commands.txt

echo "Count: $(wc -l < /tmp/stateless_commands.txt)"
cat /tmp/stateless_commands.txt
echo ""

# 3. Get source file locations for each command type
echo "--- Source locations ---"
echo ""
echo "Stateful commands with source files:"
while IFS= read -r cmd; do
  grep -rn "$cmd" --include="*.rs" | grep -vE '(//|//!|\$cmd|<Type>|MyCmd|MyCommand)' | head -1 | sed 's|'"$REPO_ROOT"'||' 
done < /tmp/stateful_commands.txt | paste -d'\t' /tmp/stateful_commands.txt - 2>/dev/null || true

echo ""
echo "Stateless commands with source files:"
while IFS= read -r cmd; do
  grep -rn "$cmd" --include="*.rs" | grep -vE '(//|//!|\$cmd|<Type>|MyCmd|MyCommand)' | head -1 | sed 's|'"$REPO_ROOT"'||' 
done < /tmp/stateless_commands.txt | paste -d'\t' /tmp/stateless_commands.txt - 2>/dev/null || true

echo ""
echo "=== TOTALS ==="
STATEFUL=$(wc -l < /tmp/stateful_commands.txt)
STATELESS=$(wc -l < /tmp/stateless_commands.txt)
COMBINED=$((STATEFUL + STATELESS))
echo "Stateful: $STATEFUL"
echo "Stateless: $STATELESS"  
echo "Combined unique: $COMBINED"

# Cleanup
rm -f /tmp/stateful_commands.txt /tmp/stateless_commands.txt
