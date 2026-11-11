# Hourly canary publication: staged activation

The enabled dispatcher runs after successful `Continuum Rust Tests` push runs on
canary, after successful `core-binaries` publication dispatches, and hourly, for
the latest unpublished canary revision. These events use the same admission script
and concurrency group. The successful publisher calls admission directly from its
final job, after build, verification, script tests and pruning succeed. It verifies
the current run identity/attempt and complete exact-key manifests before excluding
only that finishing run from the active-publication check. Every other active run
still blocks. The next build queues behind the existing non-cancelling canary
publication group until this publisher exits. This replaces the publication
`workflow_run` trigger; successful Rust checks retain their trigger.
Publication completion rechecks the latest key when its
earlier wake skipped behind an active build; complete manifests make that recheck
a no-op. PR/fork runs, failures and manually started test runs do not trigger
publication. Failed publication waits for the next ordinary wake rather than
creating an immediate retry loop. All automatic events obey
`CORE_BINARIES_HOURLY_ENABLED`; disabling it retains manual dry-run/dispatch.
`workflow_run`, like cron, requires this workflow on the repository default branch.
The hourly event still recovers missed completions; a delayed cron no longer holds
a tested merged fix until the next scheduled wake.
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
   Verify the receipt names canary's current full build key and correct publication state.
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
build on a guess. Any queued/running publication prevents another dispatch, except
the verified finishing publisher performing its own completion handoff. An incomplete
finishing release refuses handoff rather than creating a retry loop.
Manual `core-binaries` dispatch remains available independently for urgent repair.
It requires a full `build_key` selected from canary history. The dispatcher and
consumer share `core/continuum-cli-lifecycle/build-inputs.txt`: the newest commit
touching those paths is the key, so documentation-only tips do not rename binaries.
The dispatcher targets ref `canary` to preserve serial workflow concurrency, and
passes the immutable key as an input. Before building, every platform verifies
that the key is a build-input commit in canary history, checks it out, and updates
submodules. The binary stamp, package manifest, release tag and republish job all
name that key. A racing merge cannot change the selected revision. Consumer checks
continue to apply to the same build key. Workflow concurrency preserves one
active publication and GitHub's newest pending run, without cancellation starvation.

Verified prerequisite (2026-10-22): the repository default branch was `canary`.
Dry-run 39838376645's two jobs succeeded and its receipt reported full SHA
`b1a2b451b3a7dded517a3c54009335a7a78b13f6`, `dryRun: true`, `dispatched: false`.
The run subsequently reached completed/success. Preserve any active
publication across the switch. If scheduler recovery is needed, manual publication
remains available while a reviewed rollback restores push publication.
