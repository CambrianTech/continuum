# integration/recovery — Batched Integration Lane

## Purpose

`integration/recovery` is a deliberate integration branch where multiple related merges are assembled together before promoting to `canary`. It reduces canary churn by batching changes into single promotions rather than each merge triggering its own canary deployment.

## Why This Exists

Before this lane was created (2026-10-28), every merge to `canary` triggered a full deployment cycle: build, deploy, verify. When several related fixes landed in the same night (e.g., the tick-on-failure fix and verb descriptions on the same evening), each would independently promote — wasting CI time, doubling fleet disruption, and increasing the chance of catching only one half of a paired change.

The lane batches those merges so they promote as one unit:
1. Merges land on `integration/recovery` (not directly on `canary`)
2. A batch is considered ready when all related work has landed
3. The entire batch promotes to `canary` together via the automated promotion workflow

## How It Works

### Landing Work
Related changes merge into `integration/recovery`. There's no separate "gate" branch or manual approval — merges land directly here, just as they would on `main`.

### Promotion
The `.github/workflows/promote-recovery-to-canary.yml` workflow runs every 6 hours (plus by manual dispatch). When it detects commits on recovery that aren't yet on canary:

1. It checks the recovery tip's CI status (own suites only, same logic as `promote-main.yml`)
2. If green and ahead of canary → creates a PR that squash-merges all un-promoted commits into canary
3. If red or pending → refuses to promote, logs the reason

### Staleness Reporting
The workflow always reports staleness when detected: `"N commits on integration/recovery not yet promoted to canary (oldest: Mh ago)"`. This is visible in CI logs and was designed so that nobody has to go looking for these — the system announces them.

## Promotion Order Constraint

There is **one** ordering rule: land, measure under 12100 agentic surface, then promote. The tip's measured agentic surface at the new commit must be under 12100 before any promotion proceeds. This constraint was established during the 12100 ceiling analysis and applies to all promotions from this lane.

## Operational Notes

- **Don't retire this branch.** It is doing the job it was created for: reducing canary churn by batching related changes.
- **The documentation gap was the real problem**, not the branch itself. Before this doc, `integration/recovery` existed in the repo but had no explanation — any future reader (or a compacted persona) would see an undocumented branch and wrongly conclude it was abandoned or accidental.
- **Tighter cadence than promote-main.** Recovery merges happen in batches rather than continuously, so the promotion runs every 6 hours instead of daily.

## Related Files

| File | Purpose |
|------|---------|
| `.github/workflows/promote-recovery-to-canary.yml` | Automated promotion workflow (recovery → canary) |
| `.github/workflows/promote-main.yml` | Main promotion workflow (canary → main) |
| `tools/scripts/track-canary.sh` | Node-level canary tracking and deployment |

## History

- **2026-10-28:** Lane created by Joel, coordinated with Astra. Five of seven nightly merges landed here overnight — the gap was that nobody noticed because there was no automated path to canary and no documentation.
- **Card bd0d725b:** Identified the gap (no automation path), verified intent (not drift or abandonment), delivered promotion workflow + this documentation.
