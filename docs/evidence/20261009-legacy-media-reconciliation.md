# Idle legacy media recovery in the public Windows installer

Card: `2041ea54-b1cf-4778-9c3c-50504454fec0`.

## Observed failure

The verified `2c2ad67c8` public preparation could not choose an inactive core
slot. Registration still protected service-a, while a legacy service-b bridge
held its executable mapped. The old bridge had only a loopback listener and no
observed TCP clients or UDP endpoints; its original parent had exited. The
unelevated caller could not query its image (`OpenProcess` error 5). No socket
probe or manual termination was performed. A generic probe is unsafe here:
this bridge accepts one client, then exits when that client's stream ends.

## Shared correction

`New-CoreServiceRelease` first tries every eligible free slot. If the only
remaining blocker in one otherwise inactive, unregistered slot is its bridge,
normal `install.ps1` invokes the new installed-media responsibility through
the existing elevation session. `-PrepareOnly` refuses before elevation or any
connection. Both prebuilt and explicit developer installs use this owner.

The consented child validates a digest-bound plan, the exact non-reparse slot
path and image hash, and creation time through a held process handle. The
handle has query/wait rights, **no terminate right**. TCP/UDP inspection must
show only that process's expected loopback listener. A zero-command connection
and immediate EOF then ask the unused single-client bridge to exit naturally.
Exit is bounded; there is no force-termination fallback.

The install lease does not exclude Scheduler startup. If another core wins
accept, closing our queued second stream cannot close that core's first stream.
The bridge remains running and reconciliation refuses. Registration and file
locks are checked again before staging; a changed registration preserves the
candidate files. No new slots, process manager, polling daemon, manual state
repair, or parallel elevation owner was added.

## Validation

Windows PowerShell 5.1, existing fixture files:

- `windows-process.test.ps1`: 11 PASS groups. Real single-client child processes
  prove unused-listener exit, existing-client refusal, and a deterministic core
  connection between idle observation and our EOF. The preserved first stream
  remains writable and exits normally only when its owner disconnects. Image,
  creation, hash, endpoint and plan-digest mismatches refuse before connection.
  Actual CIM creation precision and structured elevated argv are checked;
  elevation itself is replaced in this fixture, so no UAC is claimed.
- `windows-service.test.ps1`: 36 PASS groups. Existing slot scenarios now cover
  PrepareOnly preservation, normal installer opt-in, successful inactive-slot
  retry and registration change during reconciliation. Existing registration,
  prepared release, engine, media startup and recovery scenarios remain intact.
- The published `2c2ad67c8` CLI still passes the existing actual preparation JSON
  and diagnostic-only supervisor PowerShell scenarios in the extended fixture.
  No production core starts during this rehearsal. The existing change-scope
  fixture passes 5 tests including the new shared installer module classification.

Raw local receipts:
`C:/Users/joelt/.continuum/state/team-proof-20260921/20261009-legacy-media-process-test.log`
and `20261009-legacy-media-service-test.log`; published CLI rehearsal:
`20261009-legacy-media-published-test.log`. No Cargo build was required.

The real legacy bridge, tasks, installed receipts and serving state were not
changed. Publication and actual consented public installation remain required;
these tests do not establish service adoption or Kimi recovery.
