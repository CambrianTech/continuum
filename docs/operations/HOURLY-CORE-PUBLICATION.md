# Hourly canary publication: staged activation

This change prepares an hourly admission/dispatch workflow. It does not yet reduce
push-triggered builds: `core-binaries.yml` remains unchanged during phase 1.
No service or deployment ledger changes are involved.

1. Review and merge the dispatcher, script and fixtures to canary. Source fixtures
   need only Node; do not run the three-platform build to test scheduling.
2. Through the normal reviewed main-promotion process, place the scheduler YAML
   **and its fixture/script dependencies** on the default branch. GitHub cron uses
   default-branch YAML. Merging to canary alone does not activate it.
3. Manually dispatch `core-binaries-hourly` on the default branch with `dry_run=true`.
   Verify the receipt names canary's current full SHA and correct publication state.
   This reads GitHub and starts no build. Keep `CORE_BINARIES_HOURLY_ENABLED` unset.
4. Prepare a separate reviewed phase-2 change removing the canary `push` trigger from
   `core-binaries.yml` and updating the lifecycle BUILD_INPUTS workflow-contract test
   (which currently requires push paths). Retain `workflow_dispatch` for urgent repairs
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
