#!/bin/bash
# check-pipefail.sh — CI ratchet: ensures all shell scripts with pipelines have set -o pipefail
# Exit 0 = all clear; exit 1 = some scripts missing pipefail (and list them)

set -e
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

count=0
missing=()

while IFS= read -r file; do
  # Skip generated files and sourced library files (lib/ is included by callers)
  case "$file" in */generated/*|*/lib/*) continue ;; esac
  
  # Check if the file has any pipe operators (not just in comments)
  # Strip comment lines, then check for | that's not inside a string or comment
  has_pipe=false
  while IFS= read -r line; do
    # Skip pure comment lines
    stripped="${line#"${line%%[![:space:]]*}"}"
    case "$stripped" in \#*) continue ;; esac
    
    # Check for actual pipeline operators (not || logical OR)
    stripped_line="${line//||/}"
    if echo "$stripped_line" | grep -q '|'; then
      has_pipe=true
      break
    fi
  done < "$file"
  
  if [ "$has_pipe" = true ]; then
    # File has pipes — check for pipefail
    if ! grep -q 'pipefail' "$file"; then
      missing+=("$file")
      count=$((count + 1))
    fi
  fi
done < <(find tools/scripts -name '*.sh' -type f)

if [ "$count" -gt 0 ]; then
  echo "FAIL: $count script(s) use pipes but lack 'set -o pipefail':" >&2
  for m in "${missing[@]}"; do
    echo "  $m" >&2
  done
  exit 1
fi

echo "OK: all scripts with pipelines have set -o pipefail"
exit 0
