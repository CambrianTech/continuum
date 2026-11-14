#!/bin/bash
# check-pipefail.sh — CI ratchet: fail if any executable script in tools/scripts/
# uses a pipeline without set -o pipefail.  Excludes lib/ (sourced libraries) and
# generated/ directories.
set -e
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

count=0
missing=""

while IFS= read -r file; do
    # Skip if already has pipefail (combined or standalone)
    grep -q 'pipefail' "$file" && continue

    # Strip || (logical OR) then check for remaining | characters in non-comment lines
    has_pipe=false
    while IFS= read -r line; do
        # Skip comment-only lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        # Remove || and check if any bare | remains
        stripped="${line//||/}"
        if [[ "$stripped" == *"|"* ]]; then
            has_pipe=true
            break
        fi
    done < "$file"

    if $has_pipe; then
        rel_path="${file#$SCRIPT_DIR/}"
        missing+="$rel_path"$'\n'
        count=$((count + 1))
    fi
done < <(find "$SCRIPT_DIR" -name '*.sh' \
         -not -path '*/lib/*' \
         -not -path '*/generated/*' \
         -type f | sort)

if [ "$count" -gt 0 ]; then
    echo "FAIL: $count script(s) use pipelines without set -o pipefail:"
    printf '%s' "$missing"
    exit 1
fi

echo "OK: all scripts with pipelines have set -o pipefail"
exit 0
