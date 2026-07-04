#!/usr/bin/env bash
# ship.sh — one command for the per-brick canary PR dance.
#
# The loop is always the same: push the feature branch, open a PR into canary,
# wait for CI, squash-merge, delete the branch, sync local canary. Doing it by
# hand (11× in one session) burns energy on ceremony. This is the leverage:
# build the automation, then every brick costs one line.
#
# Usage (run while ON a feature branch with your commits already made):
#   scripts/ship.sh                       # PR title/body filled from your commits
#   scripts/ship.sh "feat(x): title"      # explicit title, body from commits
#   scripts/ship.sh "title" "body text"   # explicit title + body
#
# Safety: refuses to run from canary/main; requires commits ahead of base;
# the squash-merge blocks on branch protection (required CI), so a red PR
# CANNOT be merged — the script fails loud instead of forcing it. Never uses
# --no-verify, never --admin. Base defaults to canary (override: SHIP_BASE).
set -euo pipefail

BASE="${SHIP_BASE:-canary}"
branch="$(git rev-parse --abbrev-ref HEAD)"

if [[ "$branch" == "$BASE" || "$branch" == "main" ]]; then
  echo "ship: refusing to ship from protected branch '$branch' — cut a feature branch first." >&2
  exit 1
fi

git fetch -q origin "$BASE"
ahead="$(git rev-list --count "origin/${BASE}..HEAD")"
if [[ "$ahead" -eq 0 ]]; then
  echo "ship: no commits ahead of origin/${BASE} — nothing to ship." >&2
  exit 1
fi

echo "ship: pushing ${branch} (${ahead} commit(s) ahead of ${BASE})…"
git push -u origin "$branch" >/dev/null 2>&1

# keep local base honest so the next `git checkout $BASE` is clean
git branch -f "$BASE" "origin/${BASE}" 2>/dev/null || true

# open the PR (explicit title/body, else fill from the branch's commits)
if [[ $# -ge 1 ]]; then
  gh pr create --base "$BASE" --head "$branch" --title "$1" --body "${2:-}" >/dev/null
else
  gh pr create --base "$BASE" --head "$branch" --fill >/dev/null
fi
echo "ship: PR opened → $(gh pr view "$branch" --json url -q .url)"

# squash-merge. Branch protection enforces required CI, so this blocks until
# checks pass — retry while they're pending, fail loud on any real error.
err="$(mktemp)"
merged=0
for i in $(seq 1 18); do
  if gh pr merge "$branch" --squash --delete-branch >/dev/null 2>"$err"; then
    merged=1; break
  fi
  if grep -qiE "not mergeable|required|checks|pending|state|expected|not yet" "$err"; then
    echo "ship: CI not green yet (attempt ${i}/18) — waiting…"
    sleep 12
    continue
  fi
  echo "ship: merge failed — real error, not pending:" >&2
  cat "$err" >&2
  rm -f "$err"
  exit 1
done
rm -f "$err"

if [[ "$merged" -ne 1 ]]; then
  echo "ship: gave up waiting for CI. Inspect: gh pr checks ${branch}" >&2
  exit 1
fi

git checkout "$BASE" -q
git pull --ff-only -q
echo "ship: done — ${branch} squash-merged to ${BASE}; local ${BASE} synced."
