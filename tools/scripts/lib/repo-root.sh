#!/bin/bash
# repo-root.sh — shared helper. Source this, then $REPO_ROOT is set.
#
# Usage:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/repo-root.sh"
#   cd "$REPO_ROOT/src"
#
# Works from any CWD. Derives from the location of this file, then walks up
# to find the nearest parent directory containing `docker-compose.yml`.
# Exports REPO_ROOT. If you source this multiple times it's idempotent.

# Already set by an outer script? Trust it.
if [ -n "${REPO_ROOT:-}" ] && [ -f "$REPO_ROOT/docker-compose.yml" ]; then
  return 0 2>/dev/null || true
fi

# Resolve this file's directory, follow symlinks correctly.
_repo_root_self="${BASH_SOURCE[0]}"
while [ -L "$_repo_root_self" ]; do
  _repo_root_dir="$(cd "$(dirname "$_repo_root_self")" && pwd)"
  _repo_root_self="$(readlink "$_repo_root_self")"
  case "$_repo_root_self" in /*) ;; *) _repo_root_self="$_repo_root_dir/$_repo_root_self" ;; esac
done
_repo_root_dir="$(cd "$(dirname "$_repo_root_self")" && pwd)"

# Walk up from scripts/lib/ looking for the root marker (docker-compose.yml).
_candidate="$_repo_root_dir"
while [ "$_candidate" != "/" ]; do
  if [ -f "$_candidate/docker-compose.yml" ] && [ -d "$_candidate/src" ]; then
    export REPO_ROOT="$_candidate"
    unset _repo_root_self _repo_root_dir _candidate
    return 0 2>/dev/null || true
  fi
  _candidate="$(dirname "$_candidate")"
done

# Walked to / and found nothing.
echo "❌ repo-root.sh: could not locate continuum repo root (no docker-compose.yml found walking up from $_repo_root_dir)" >&2
unset _repo_root_self _repo_root_dir _candidate
return 2 2>/dev/null || exit 2
