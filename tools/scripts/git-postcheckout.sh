#!/bin/bash
# Git post-checkout hook — stamps Rust source file mtimes after branch switches
# to prevent cargo from serving stale artifacts from its incremental cache.
#
# When git checks out files during a branch switch, it preserves their original
# mtimes (or sets them to the commit date). If those mtimes match or are older
# than what cargo's incremental cache recorded for the previous branch's build,
# cargo may reuse stale .rlib/.so artifacts that contain code from the old branch.
# This manifests as phantom errors: missing fields in derived structs, unresolved
# imports, etc. — all fixed by touching the source files (confirming it's a mtime
# issue, not actual code bugs).
#
# The fix: stamp every .rs file with current time after checkout. Cargo's content-
# based fingerprint will still catch if nothing actually changed, but the fresh
# mtime forces re-evaluation of incremental dependencies.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# post-checkout receives: $1=prev HEAD, $2=new HEAD, $3=branch-flag (1=checkout, 0=file)
BRANCH_FLAG="${3:-0}"

if [ "$BRANCH_FLAG" -ne 1 ]; then
    # Not a branch checkout — skip. We only need to stamp on branch switches,
    # not when checking out individual files or during sparse-checkout operations.
    exit 0
fi

# Stamp all Rust source files with current mtime so cargo's incremental cache
# re-evaluates dependencies after every branch switch. This is cheap (just a
# filesystem touch) and prevents the stale artifact problem entirely.
find "$REPO_ROOT/core" -name '*.rs' -type f -exec touch {} + 2>/dev/null || true

exit 0
