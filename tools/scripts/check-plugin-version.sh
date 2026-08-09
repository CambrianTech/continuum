#!/usr/bin/env bash
# check-plugin-version.sh — a plugin's content must never change without its
# version changing.
#
# ## The failure this guards
#
# Claude Code plugin updates are VERSION-BASED. A marketplace install copies the
# plugin to ~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/ and pins it.
# `claude plugin update` compares the DECLARED version in plugin.json against the
# installed one — it does not look at content. So if the scripts change and the
# version does not, every installed copy answers:
#
#     ✔ memory-bridge is already at the latest version (0.1.0).
#
# ...forever, and the fix reaches nobody. `git pull` does not update a plugin.
#
# Measured 2026-08-09: memory-bridge sat at 0.1.0 since 2026-07-25 while its
# scripts gained a persona-id cache and a whole session-capture.sh. The installed
# copy on BigMama had NEITHER — automatic per-turn memory capture had never run
# once on that machine, while the repo held working code and the README said the
# bridge was live. Bumping 0.1.0 → 0.2.0 propagated all of it in one command.
#
# This is [[silently-unwired-capability]] in its deployment form: a fix that never
# reaches the executing copy is identical to a fix never written. The repo keeps
# looking correct, because it is.
#
# ## The gate
#
# If a commit touches any file under a plugin directory, that plugin's
# `.claude-plugin/plugin.json` version MUST also change. Same shape as the
# install-manifest projection guard: the generated/consumed artifact and its
# source cannot drift apart silently.
#
#   tools/scripts/check-plugin-version.sh            # staged changes (pre-commit)
#   tools/scripts/check-plugin-version.sh <base-ref> # a range (CI)
#
# Exits 0 when clean or when no plugin files changed; 1 (loud) on a missed bump.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

PLUGIN_ROOT="tools/plugins"
[ -d "$PLUGIN_ROOT" ] || exit 0   # no plugins in this tree — nothing to guard

BASE="${1:-}"
if [ -n "$BASE" ]; then
  CHANGED="$(git diff --name-only "$BASE"...HEAD -- "$PLUGIN_ROOT" 2>/dev/null)"
  DESC="changed since $BASE"
else
  CHANGED="$(git diff --cached --name-only -- "$PLUGIN_ROOT" 2>/dev/null)"
  DESC="staged"
fi

[ -n "$CHANGED" ] || exit 0

# Which plugins were touched? A path is tools/plugins/<name>/... — the marketplace
# manifest at tools/plugins/.claude-plugin/ has no <name> segment, so the `.`-prefixed
# entry is filtered out rather than treated as a plugin called ".claude-plugin".
PLUGINS="$(printf '%s\n' "$CHANGED" \
  | sed -n "s#^$PLUGIN_ROOT/\([^/.][^/]*\)/.*#\1#p" | sort -u)"

[ -n "$PLUGINS" ] || exit 0

FAILED=0
for plugin in $PLUGINS; do
  manifest="$PLUGIN_ROOT/$plugin/.claude-plugin/plugin.json"
  if [ ! -f "$manifest" ]; then
    echo "✗ $plugin: no $manifest — a plugin without a manifest cannot be versioned or installed" >&2
    FAILED=1
    continue
  fi
  # Did the version line itself change in this same set? Compare the manifest's
  # version before and after rather than trusting that the manifest was touched:
  # editing the description is not a release.
  if [ -n "$BASE" ]; then
    before="$(git show "$BASE:$manifest" 2>/dev/null)"
    after="$(git show "HEAD:$manifest" 2>/dev/null)"
  else
    before="$(git show "HEAD:$manifest" 2>/dev/null)"
    after="$(cat "$manifest" 2>/dev/null)"
  fi
  # A brand-new plugin has no `before` — nothing to bump from, so it passes.
  [ -n "$before" ] || continue
  v_before="$(printf '%s' "$before" | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
  v_after="$(printf '%s' "$after"  | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
  if [ -z "$v_after" ]; then
    echo "✗ $plugin: $manifest declares no \"version\" — plugin update has nothing to compare" >&2
    FAILED=1
    continue
  fi
  if [ "$v_before" = "$v_after" ]; then
    echo "✗ $plugin: files $DESC but version is still \"$v_after\"." >&2
    echo "    Plugin updates are VERSION-based, not content-based. Every installed copy will" >&2
    echo "    report 'already at the latest version' and keep running the OLD scripts — your" >&2
    echo "    change reaches nobody, and nothing reports the gap." >&2
    echo "    Fix: bump \"version\" in $manifest" >&2
    FAILED=1
  fi
done

if [ "$FAILED" -ne 0 ]; then
  echo "" >&2
  echo "plugin-version gate failed. See the header of tools/scripts/check-plugin-version.sh" >&2
  exit 1
fi

exit 0
