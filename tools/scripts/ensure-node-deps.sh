#!/bin/bash
# ensure-node-deps.sh — install the npm workspace when it is missing or stale.
#
# ## Why this exists
#
# `install.sh` runs `npm install` ONCE, at install time. Nothing re-ran it when
# the manifest moved. So a contributor who installed in June and pulled in August
# had a `node_modules` that silently did not match its own `package.json` — and
# the first symptom was every client spec file failing at COLLECTION with:
#
#     Failed to load url @continuum/chat-view. Does the file exist?
#
# which points a newcomer at missing SOURCE, not at their missing deps. Measured
# 2026-08-08 on a real checkout: tree from Jun 17, manifest from Aug 5, `lit` and
# every `@continuum/*` workspace package absent, seven spec files dead.
#
# ## Why HERE and not in the start path
#
# `start-server.sh` is headless Rust by doctrine — "No Node, no TS, no widgets."
# A dependency guard wired there would drag npm into a runtime path that exists
# precisely to avoid it. So this hangs off the CLIENT scripts only (`pre*` hooks
# on dev:web / build:clients / test:clients / typecheck:clients). Someone who
# only ever runs the core pays nothing and never sees this file run.
#
# ## Why mtime and not a checksum
#
# npm writes `node_modules/.package-lock.json` when it materialises the tree, so
# "tree older than lockfile" is exactly the question worth asking, answerable
# without parsing either file or shelling out to npm. A checksum would be more
# precise about CONTENT and no more precise about the thing that actually breaks
# people, which is a tree that predates a manifest change.
#
# Skipped entirely in CI: `npm ci` there is authoritative and already ran, and a
# second install would only add minutes and a chance to disagree with the
# lockfile.

set -e

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

# CI installs from the lockfile deliberately; never second-guess it.
if [ -n "${CI:-}" ]; then
  exit 0
fi

# Escape hatch for anyone deliberately hand-managing their tree.
if [ -n "${CONTINUUM_SKIP_DEP_CHECK:-}" ]; then
  exit 0
fi

INSTALLED_MARKER="node_modules/.package-lock.json"
REASON=""

if [ ! -d node_modules ]; then
  REASON="node_modules is missing"
elif [ ! -f "$INSTALLED_MARKER" ]; then
  # A node_modules with no marker was not written by a modern npm install —
  # treat it as unknown rather than assume it is good.
  REASON="node_modules has no install marker"
elif [ package-lock.json -nt "$INSTALLED_MARKER" ]; then
  REASON="package-lock.json is newer than the installed tree"
elif [ package.json -nt "$INSTALLED_MARKER" ]; then
  REASON="package.json is newer than the installed tree"
fi

if [ -z "$REASON" ]; then
  exit 0
fi

# Loud, never silent. A guard that fixes things without saying so teaches the
# operator that installs are magic, and hides a real signal (a lockfile moving
# under them) that is sometimes worth knowing about.
echo "deps: $REASON — running npm install"
npm install --silent
echo "deps: workspace up to date"
