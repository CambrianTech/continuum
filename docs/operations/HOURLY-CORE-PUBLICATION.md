# Hourly canary publication: staged activation

The publisher is dispatched hourly for the latest unpublished canary revision.
`core-binaries.yml` retains urgent manual dispatch and has no automatic push trigger.
No service or deployment ledger changes are involved.

1. Review and merge the dispatcher, script and fixtures to canary. Source fixtures
   need only Node; do not run the three-platform build to test scheduling.
2. Read the live default branch with `gh repo view --json defaultBranchRef`.
   GitHub cron uses default-branch YAML. If that branch is `canary`, the merge in
   step 1 already installs the scheduler and dependencies there; no main promotion
   is needed. Otherwise use the normal reviewed promotion process to place the
   scheduler YAML **and its fixture/script dependencies** on the reported default
   branch before proceeding. Do not assume the default branch is `main`.
3. Manually dispatch `core-binaries-hourly` on the default branch with `dry_run=true`.
   Verify the receipt names canary's current full SHA and correct publication state.
   This reads GitHub and starts no build. Keep `CORE_BINARIES_HOURLY_ENABLED` unset.
4. Review the phase-2 removal of the canary `push` trigger from `core-binaries.yml`
   and the updated lifecycle workflow contract. Retain `workflow_dispatch` for urgent repairs
   and the existing non-cancelling publication concurrency. Do not enable
   `cancel-in-progress`: frequent merges previously cancelled every long build.
5. Coordinate the switch: enable repository variable `CORE_BINARIES_HOURLY_ENABLED=true`
   after the default-branch dry-run succeeds, then integrate phase 2. Observe a real
   hourly event and publication. Brief overlap is safe: admission skips a SHA already
   queued/running, and the publisher still serializes canary runs. If the scheduler
   fails, retain/restore push publication and disable the variable.

The dispatcher checks all three platforms' manifest, archive and checksum assets,
including full manifest git SHA, before declaring a revision published. Missing or
partial publication is eligible; API errors fail visibly instead of launching a
build on a guess. An existing queued/running publication for that SHA is skipped.
Manual `core-binaries` dispatch remains available independently for urgent repair.
The dispatcher always targets ref `canary`; GitHub resolves it at dispatch time, so
a racing merge may produce a newer SHA than the admission receipt. The build's own
receipt is authoritative for its revision. Workflow concurrency preserves one
active publication and GitHub's newest pending run, without cancellation starvation.

Verified prerequisite (2026-10-22): the repository default branch was `canary`.
Dry-run 39838376645's two jobs succeeded and its receipt reported full SHA
`b1a2b451b3a7dded517a3c54009335a7a78b13f6`, `dryRun: true`, `dispatched: false`.
Confirm the current run conclusion before enabling the variable; preserve any active
publication across the switch. If scheduler recovery is needed, manual publication
remains available while a reviewed rollback restores push publication.
