# Promoting Integration and Recovery to Canary

## Overview

This repository uses a promotion pipeline where feature work lands on `integration` and critical fixes land on `recovery`, then gets promoted automatically (or manually) to `canary` for wider testing.

The automated path is provided by the **promote-integration-recovery-to-canary** GitHub Actions workflow, which runs:
- On a schedule at 10:37 UTC daily
- Manually via `workflow_dispatch` with merge toggles

## How It Works

### When to Use

| Situation | Action |
|-----------|--------|
| Feature PRs land on `integration` and you want them in canary | The workflow auto-promotes on schedule; or trigger manually |
| Critical fix lands on `recovery` and needs immediate testing in canary | Trigger the workflow manually with merge enabled for recovery only |
| Both need promotion | Default dispatch merges both; set merge toggles to false to just open PRs |

### The Promotion Mechanism

1. Detect: Compares each source branch (`integration`, `recovery`) against `canary`. If a branch doesn't exist or its tree already matches canary's, it is skipped with a notice.
2. Verify: Runs the same tip-green check used by `promote-main.yml` — checks whether the source branch's own push has all required CI suites passing (excluding this workflow and core-binaries).
3. Promote: Creates a promote branch via `git checkout -B <branch> refs/heads/<source>` then `git merge -s ours --no-ff origin/canary`, recording canary's history while adopting the source tree byte-for-byte.
4. PR: Opens (or updates) a PR from the promote branch into `canary`. The commit message records which SHA was promoted and lists all commits being brought over.
5. **Merge** (optional): On schedule runs, auto-merges the PR when green. Manual dispatch can skip merging to inspect the PR first.

### Token Requirements

- **GITHUB_TOKEN**: Sufficient if `canary` has no branch protection rules that block the workflow's bot commit.
- **PROMOTE_TOKEN secret**: Required if `canary` is protected. This should be a fine-grained PAT with `contents: write` and `pull-requests: write`. Its check runs will pass PR checks so auto-merge can arm.

## Branch Protection Considerations

If you add branch protection to `canary`:
1. Create the `PROMOTE_TOKEN` secret (fine-grained PAT)
2. Set up a required status check for "promote-integration-recovery-to-canary" on `canary` — this workflow's runs will then count as passing checks when merging into canary via its PRs

## Manual Promotion

For urgent recovery promotions:
```bash
gh workflow run promote-integration-recovery-to-canatory.yml \
  -f merge_integration=false \
  -f merge_recovery=true
```

This opens the recovery→canary PR and merges it immediately. Set `merge_integration` to control whether the integration PR also gets merged in the same run.

## Comparison with promote-main.yml

| Aspect | promote-main | integrate-recovery-to-canary |
|--------|--------------|------------------------------|
| Source branch | `main` (single) | `integration` + `recovery` (dual) |
| Target | `canary` | `canary` |
| Run trigger | Daily 09:37 UTC | Daily 10:37 UTC |
| Merge behavior | Always merges when green | Manual dispatch toggles per-source; schedule always merges |
| Non-deploy exclusion | `.github/workflows/promote-main.yml,.github/workflows/core-binaries.yml` | `.github/workflows/promote-integration-recovery-to-canary.yml,.github/workflows/core-binaries.yml` |

## Troubleshooting

**"nothing to promote"** — The source branch either doesn't exist or its tree already matches canary's. No action needed; the workflow exited cleanly.

**"tip checks not green"** — The source branch has a failing CI run. Fix the failing check, push again, and wait for the next scheduled promotion (or trigger manually).

**PR left open after schedule run** — The token could not auto-merge. This typically means `canary` is protected and only a PAT can merge. Set the `PROMOTE_TOKEN` secret.

**"the promote tree is not integration's/recovery's tree"** — Something went wrong during the `-s ours` merge. Check git logs for conflicts or unexpected state; this should never happen on clean branches but serves as a safety check.
