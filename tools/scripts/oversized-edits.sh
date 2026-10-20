#!/usr/bin/env bash
# NEW RULE (Joel, 2026-10-20): "No one can edit a large file." Every Rust file a change adds or
# modifies is at most 1,000 lines at its head; the only change that touches a monolith is the one
# that breaks it apart. Usage: oversized-edits.sh <base-ref>. Exit 1 names every offender.
set -euo pipefail
base="$1"
bad=0
while IFS=$'\t' read -r added deleted path; do
  case "$path" in *.rs) ;; *) continue ;; esac
  case "$path" in core/vendor/*|*/vendored/*) continue ;; esac
  [ -f "$path" ] || continue
  lines=$(awk 'END{print NR}' "$path")  # counts a final line with no newline too
  if [ "$lines" -gt 1000 ]; then
    echo "::error file=$path::$path is $lines lines and this change edits it (+$added -$deleted). No one edits a large file: break it apart by owner in this change (pure moves, one owner per file) so it ends at most 1,000 lines."
    bad=1
  fi
done < <(git diff --numstat --no-renames "$base"...HEAD)  # a rename+edit is checked at its destination
exit $bad
